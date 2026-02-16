# Fathom Architecture

## Overview
Fathom is a session-oriented agent runtime with a gRPC server and TUI client.

- Server manages many sessions concurrently.
- Each session is processed by a single actor task for deterministic ordering.
- Clients can attach to one or more sessions and consume event streams.
- Agent input is trigger-based, not direct message-based.
- Agent intelligence is backed by OpenAI Responses API.
- Assistant user-facing output supports streaming from server to client.
- Server synthesizes authoritative time context (UTC + server-local timezone) for agent turns.

## Core Concepts

### Session
A session is the unit of conversation and orchestration.

- Accepts triggers from users, tasks, heartbeat, and cron.
- Runs exactly one agent turn at a time.
- Maintains:
  - immutable profile copies (`agent_profile_copy`, `participant_user_profiles_copy`)
  - queued triggers
  - conversation history
  - task registry
  - engaged environment set (`engaged_environment_ids`)
  - environment state snapshots (`environment_snapshots`)
  - in-flight action hints for prompt context

### Trigger
Trigger variants:

- `UserMessage`
- `TaskDone`
- `Heartbeat`
- `Cron`
- `RefreshProfile`

Turn cut policy is snapshot-based:

1. At turn start, all currently queued triggers are snapshotted.
2. New triggers arriving during the turn remain queued for the next turn.

### Agent Turn
Per turn:

1. Consume trigger snapshot.
2. Build prompt context from profile copies + trigger snapshot + recent history.
3. Invoke OpenAI Responses API in streaming mode with action calling (`env__action`) and native assistant output.
4. Stream assistant text deltas into `AssistantStream` events.
5. Dispatch action calls immediately as background tasks.
6. Emit session events.
7. Flush trigger snapshot and assistant outputs into history atomically.

### Task
Tasks are background jobs created by agent actions.

- States: `Pending`, `Running`, `Succeeded`, `Failed`, `Canceled`.
- Task completion re-enters the session as `Trigger::TaskDone`.
- One model action call maps to one background task.
- Canonical action ID format: `env__action` (examples: `filesystem__read`, `system__get_time`).
- Action dispatch model:
  - Session actor routes each task to the target environment actor.
  - Environment actor may execute independent actions in parallel.
  - Commit order is deterministic per environment sequence.
  - `TaskDone` is emitted after commit finalization (success or failure).
- Implemented filesystem actions execute as real background jobs:
  - `filesystem__get_base_path()`
  - `filesystem__list(path)`
  - `filesystem__read(path)`
  - `filesystem__write(path, content, allow_override)`
  - `filesystem__replace(path, old, new, mode)`
- Assistant output policy:
  - User-facing messages come from native assistant model output (not a special action).
  - Streaming uses `AssistantStream`; finalized content uses matching `AssistantOutput(stream_id=...)`.
- History transformation policy:
  - `task_started` and `task_finished` are recorded as distinct history events.
  - each task history entry includes `canonical_action_id`, `environment_id`, and `action_name`.
  - Task args/results are stored in history as truncated previews with byte/line cutoff metadata and lookup references.
  - Agent can query full payloads with `system__get_task_payload`.
- Time context policy:
  - Each turn snapshot includes `time_context` (`utc_rfc3339`, `local_rfc3339`, `local_timezone_name`, `local_utc_offset`, `generated_at_unix_ms`).
  - Each turn snapshot includes `activated_environments` (`id`, `name`, `description`).
  - `system__get_context` includes the same `time_context` shape.
  - `system__get_time` returns refreshed server-clock time when the model needs newer values mid-session.
  - `system__describe_environment(env_id)` returns detailed environment docs for activated environments.

### Filesystem Path Model
Filesystem actions use plain relative paths resolved from the filesystem environment base path.

- Examples: `notes/today.md`, `src/main.rs`, `.`
- Rejected: absolute paths, URI schemes (`://`), and paths that escape base path (`../../...`)

Profile content is not exposed as pseudo-files via filesystem actions. Profile and memory data are accessed through system actions such as `system__list_profiles` and `system__get_profile`.
Environment state is opaque to the agent by default. Agents inspect environment internals through explicit inspection actions (for example `filesystem__get_base_path` and `system__describe_environment`), not by raw state injection.

## Identity and Memory

Profiles are canonical global entities:

- `AgentProfile`
  - includes profile content fields for `AGENTS.md`, `SOUL.md`, `IDENTITY.md`
  - includes long-term agent memory
- `UserProfile`
  - includes profile content field for `USER.md`
  - includes long-term user memory and preferences

Sessions hold immutable copies of these profiles for deterministic replay.
`RefreshProfile` trigger updates the session-local copies explicitly.

## Event Model
Each session publishes a stream of `SessionEvent`:

- `TriggerAccepted`
- `TurnStarted`
- `TurnEnded`
- `AssistantOutput`
- `AssistantStream`
- `TaskStateChanged`
- `ProfileRefreshed`
- `AgentStream`
- `TurnFailure`

`AssistantOutput` is the canonical finalized assistant message.
`AssistantStream` is progressive output for live rendering and includes:
- `stream_id` for correlation
- `delta` text chunk
- `done` lifecycle marker

Client-side dedup policy:
- streamed assistant text is rendered progressively
- matching finalized `AssistantOutput(stream_id=...)` replaces/finalizes the same visible line
- duplicate finalized outputs for the same `stream_id` are ignored in conversation view

## Components

### Server (`fathom-server`)
- `RuntimeService` gRPC API.
- In-memory runtime state:
  - global profile stores
  - session registry
  - per-session actor loop
- Layered internal modules:
  - `agent/*`: model orchestration, prompt rendering
    - prompt/system context includes activated environment summaries (`id`, `name`, short description)
    - mutable environment snapshots are not injected directly
  - `environment/*`: environment registry, environment actors, and built-in environment definitions
    - `environment/registry.rs`: composes environments and canonical action registry
    - `environment/actor.rs`: per-environment child actor runtime with in-order commit
    - `environment/system/*`: built-in privileged system environment actions (`system__*`)
  - `session/*`: deterministic session actor + action-task orchestration
  - `session/engine/assistant_stream.rs`: native assistant text streaming and batching
  - `history/*`: structured history line transformation and preview truncation
  - `system_env/*`: runtime/profile/session/task discovery action execution
    - includes environment discovery (`system__describe_environment`) for deeper docs/capabilities/recipes
- OpenAI-backed `AgentOrchestrator` with:
  - server-defined action registry sourced from `Environment + Action` contracts
  - streaming Responses API integration
  - retry policy with backoff/jitter and `Retry-After` support

### Environment Contracts
- `fathom-env`:
  - shared environment/action contracts (`Environment`, `Action`, `ActionSpec`, `ActionOutcome`)
  - environment metadata includes `id`, `name`, and `description`
  - canonical naming helpers (`env__action`)
- `envs/fathom-env-fs`:
  - filesystem environment action instances (`get_base_path`, `list`, `read`, `write`, `replace`)
  - action schemas and validation
  - filesystem execution backend (path parsing, sandboxing, real I/O)
- System actions remain built-in in `fathom-server` because they require privileged server/runtime access.

### Client (`fathom-client`)
- gRPC client wrapper for runtime API.
- TUI runtime that:
  - creates/upserts profiles
  - creates a session
  - subscribes to session events
  - enqueues user and heartbeat triggers asynchronously (UI loop never blocks on RPC round-trips)
  - transforms all inbound events into one canonical internal `EventRecord`
  - routes the same `EventRecord` stream to all tab implementations
  - merges network stream events and async enqueue completion/status updates through one internal app event channel
  - provides local slash-command execution modules
    - each command lives in a dedicated local module under `fathom-client/src/commands/*`
    - current command inventory is intentionally small (`/heartbeat` only)
  - provides slash-command autocomplete popup in input flow
    - typing `/` with empty input opens a vertical `command - description` list
    - prefix typing (e.g. `/he`) live-filters command candidates
    - `Up/Down` navigates candidate list; `Enter`/`Tab` inserts selected command text (with trailing space) without immediate execution
    - command runs only after a subsequent submit (`Enter`)
- Tab architecture:
  - `Conversation` tab:
    - chat-oriented projection only (user + assistant conversation lines)
    - user lines are derived from local send actions
    - assistant lines are rendered inline and updated smoothly during streaming
    - internal/system diagnostics are excluded from this tab
  - `Events` tab: full-fidelity debug event stream
  - tab switching via `Shift+Tab`
  - input remains interactive while assistant streaming is in progress

### CLI (`fathom`)
- `fathom server --addr ...`
- `fathom client --server ...`
- `cargo run` starts server + client in a combined local flow

## Current Scope
This implementation is intentionally in-memory and bootstrap-focused.
Persistence, authorization/approval policy, and real environment backends can be layered on top of this runtime contract.

## Environment
- Required: `OPENAI_API_KEY`
- For local development, use `direnv` or equivalent shell environment loader.
