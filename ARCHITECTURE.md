# Fathom Architecture

## Overview
Fathom is a session-oriented agent runtime with a gRPC server and TUI client.

- Server manages many sessions concurrently.
- Each session is processed by a single actor task for deterministic ordering.
- Clients can attach to one or more sessions and consume event streams.
- Agent input is trigger-based, not direct message-based.
- Agent intelligence flows through a provider-neutral model-adapter boundary; OpenAI Responses API is the current implementation.
- Assistant user-facing output supports streaming from server to client.
- Server synthesizes authoritative time context (UTC + server-local timezone) for agent turns.
- Model-facing behavior is defined by turn snapshot synthesis + prompt assembly over typed history and compaction summaries.
- Capability domain model currently includes `filesystem`, `brave_search`, `jina`, `shell`, and built-in `system`.
- Server writes structured diagnostics JSON logs under `.fathom/diagnostics/` for turn/invocation/task tracing.

## Core Concepts

### Session
A session is the unit of conversation and orchestration.

- Accepts triggers from users, tasks, heartbeat, and cron.
- Runs exactly one agent turn at a time.
- Uses barrier scheduling for agent invocation:
  - a turn can start only when trigger queue is non-empty and there are no in-flight actions
  - while actions are running, incoming triggers are queued (including user messages)
  - when barrier opens, queued triggers are merged into one turn snapshot
- Maintains:
  - immutable profile copies (`agent_profile_copy`, `participant_user_profiles_copy`)
  - queued triggers
  - conversation history
  - task registry
  - engaged capability-domain set (`engaged_capability_domain_ids`)
  - capability-domain state snapshots (`capability_domain_snapshots`)
  - in-flight action hints for prompt context
  - ephemeral resolved payload lookups (`pending_payload_lookups`)

### Trigger
Trigger variants:

- `UserMessage`
- `TaskDone`
- `Heartbeat`
- `Cron`
- `RefreshProfile`

Turn cut behavior is snapshot-based:

1. At turn start, all currently queued triggers are snapshotted.
2. New triggers arriving during the turn remain queued for the next turn.
3. If in-flight actions exist, trigger processing is deferred until the barrier opens.

### Agent Turn
Per turn:

1. `TurnCoordinator` opens a turn only when the barrier condition is satisfied:
   - trigger queue is non-empty
   - no in-flight actions remain
2. All queued triggers are drained into one turn snapshot.
3. Trigger preprocessing runs first:
   - `RefreshProfile` is handled on the session side
   - profile refresh emits `ProfileRefreshed` plus `SystemNotice`
   - remaining triggers become agent-facing triggers
4. `run_agent_invocation` builds one `TurnSnapshot` and one prompt bundle for the attempt.
5. `AgentOrchestrator` runs semantic attempts:
   - the initial prompt bundle is reused for diagnostics
   - one semantic retry is allowed for recoverable invalid tool-call errors
6. `ModelAdapter` streams provider output as typed `ModelDeltaEvent` items.
7. `TurnDeltaTransport` translates model deltas into `AgentStream`, `AssistantStream`, and tool-call argument lifecycle events.
8. `TurnToolDispatcher` handles validated `ActionInvocation` events:
   - queue background tasks
   - emit queued `ToolCall`
   - record dispatch diagnostics
9. Final assistant outputs are emitted as canonical `AssistantOutput` events.
10. Trigger snapshot and assistant outputs are flushed into typed history atomically.
11. Invocation and turn diagnostics are written through the invocation journal.

### Task
Tasks are background jobs created by agent actions.

- States: `Pending`, `Running`, `Succeeded`, `Failed`, `Canceled`.
- Task completion re-enters the session as `Trigger::TaskDone`.
- One model action call maps to one background task.
- Canonical action ID format: `env__action` (examples: `filesystem__read`, `system__get_time`).
- Action dispatch model:
  - Session actor routes each task to the target capability-domain actor.
  - Capability-domain actor may execute independent actions in parallel.
  - Commit order is deterministic per capability-domain sequence.
  - `TaskDone` is emitted after commit finalization (success or failure).
  - `TaskDone` triggers do not force immediate turn execution while in-flight actions remain.
- Timeout contract:
  - each action defines `max_timeout_ms` and optional `desired_timeout_ms`
  - effective timeout is resolved server-side (`desired` or `max`)
  - if `desired > max`, task fails fast with timeout-policy error
  - if execution exceeds effective timeout, task fails with timeout-exceeded error
  - timeout behavior is server/runtime controlled, not model-controlled
- Implemented filesystem actions execute as real background jobs:
  - `filesystem__get_base_path()`
  - `filesystem__list(path)`
  - `filesystem__read(path, offset_line?, limit_lines?)`
  - `filesystem__write(path, content, allow_override, create_parents?)`
  - `filesystem__replace(path, old, new, mode, expected_replacements?)`
  - `filesystem__glob(pattern, path?, max_results?, include_hidden?)`
  - `filesystem__search(pattern, path?, include?, max_results?, case_sensitive?)`
- Implemented shell action executes as real background job:
  - `shell__run(command, path?, env?)`
- Implemented Brave Search action executes as real background job:
  - `brave_search__web_search(query, count?)`
- Implemented Jina Reader action executes as real background job:
  - `jina__read_url(url)`
- Assistant output behavior:
  - User-facing messages come from native assistant model output (not a special action).
  - Streaming uses `AssistantStream`; finalized content uses matching `AssistantOutput(stream_id=...)`.
- History transformation contract:
  - history uses typed event variants, not raw JSON lines or stringly payload parsing
  - `task_started` and `task_finished` are recorded as distinct history events.
  - each task history entry includes `canonical_action_id`, `capability_domain_id`, and `action_name`.
  - Task args/results are stored in history as head/tail previews with truncation metadata and lookup references.
  - Agent can query payload chunks with `system__get_task_payload` and use offset paging (`offset`, `limit`, `next_offset`).
  - Resolved payload chunks are injected into prompt context through an ephemeral lookup buffer.
  - older history can be compacted into deterministic session summary blocks that are injected ahead of the live history window during prompt assembly
  - Ephemeral lookup buffer is cleared only when the session reaches quiescence:
    - assistant output emitted
    - no new action calls dispatched
    - no in-flight actions
    - no queued triggers
- Time context contract:
  - Each turn snapshot includes `time_context` (`utc_rfc3339`, `local_rfc3339`, `local_timezone_name`, `local_utc_offset`, `generated_at_unix_ms`).
  - Each turn snapshot includes `activated_capability_domains` (`id`, `name`, `description`).
  - `system__get_context` includes the same `time_context` shape.
  - `system__get_time` returns refreshed server-clock time when the model needs newer values mid-session.
  - `system__describe_capability_domain(capability_domain_id)` returns detailed capability-domain docs for activated capability domains.

### Filesystem Path Model
Filesystem actions use plain relative paths resolved from the filesystem capability-domain base path.

- Examples: `notes/today.md`, `src/main.rs`, `.`
- Rejected: absolute paths, URI schemes (`://`), and paths that escape base path (`../../...`)

Profile content is not exposed as pseudo-files via filesystem actions. Profile and memory data are accessed through system actions such as `system__list_profiles` and `system__get_profile`.
Capability-domain state is opaque to the agent by default. Agents inspect capability-domain internals through explicit inspection actions (for example `filesystem__get_base_path` and `system__describe_capability_domain`), not by raw state injection.

### Shell Path Model
Shell actions use plain relative directory paths resolved from the shell capability-domain base path.

- `shell__run.path` defaults to `.`
- Absolute paths, URI schemes, and escapes outside base path are rejected
- Command execution is non-interactive with runtime-managed timeout + bounded stdout/stderr capture
- Non-zero exit codes produce failed task outcomes

### Jina URL Model
Jina reader action accepts one absolute URL:

- `jina__read_url.url` must use `http://` or `https://`
- Relative URLs and non-http schemes are rejected
- Content output is markdown and may be truncated with explicit metadata

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
- `SystemNotice`
- `ToolCall`
- `AgentStream`
- `TurnFailure`

`AssistantOutput` is the canonical finalized assistant message.
`AssistantStream` is progressive output for live rendering and includes:
- `stream_id` for correlation
- `delta` text chunk
- `done` lifecycle marker

`SystemNotice` is used for internal session-side notices that should not appear as assistant chat content.

`ToolCall` is the first-class tool lifecycle stream for model-originated tool execution and currently includes:
- argument deltas
- finalized argument payloads
- queued task mapping

Client-side dedup behavior:
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
  - `agent/*`: model orchestration, prompt rendering, provider adapters
    - `agent/prompt_assembler.rs`: builds the canonical prompt bundle for one attempt
    - `agent/model_adapter.rs`: provider-neutral streaming model interface
    - `agent/openai.rs`: OpenAI Responses API adapter implementation
    - `agent/tool_catalog.rs`: session-scoped provider-visible tool catalog derived from engaged capability domains
    - prompt/system context includes activated capability-domain summaries (`id`, `name`, short description)
    - mutable capability-domain snapshots are not injected directly
  - `runtime/context_snapshot.rs`: turn-time context synthesis (runtime/session/participants/engaged capability domains/in-flight actions)
  - `capability_domain/*`: capability-domain registry, capability-domain actors, and built-in capability-domain definitions
    - `capability_domain/registry.rs`: composes capability domains and canonical action registry
    - `capability_domain/actor.rs`: per-capability-domain child actor runtime with in-order commit
    - `capability_domain/system/*`: built-in privileged system capability-domain actions (`system__*`)
- `session/*`: deterministic session actor + turn orchestration
  - barrier scheduling: triggers are drained only when no in-flight actions exist
  - `session/engine/turn/coordinator.rs`: turn gating, trigger drain, preprocessing, and finalization
  - `session/engine/turn/invocation.rs`: invocation execution and result finalization
  - `session/engine/turn/journal.rs`: invocation and turn diagnostics records
  - `session/engine/delta_transport.rs`: translates `ModelDeltaEvent` into session events and assistant streaming
  - `session/engine/tool_dispatch.rs`: action-task queueing and queued tool-call emission
  - `session/engine/assistant_stream.rs`: native assistant text streaming and batching
  - `runtime/diagnostics.rs`: structured JSON diagnostic sink
    - `sessions/<session_id>/events.jsonl` for coarse execution timeline (turns/invocations/tasks)
    - `sessions/<session_id>/invocations/invocation-<n>.json` for full per-invocation synthesized context + prompt
    - excludes high-frequency provider stream delta events from diagnostic note capture
  - `history/*`: typed history transformation, payload preview synthesis, and deterministic session compaction
  - `system_capability_domain/*`: runtime/profile/session/task discovery action execution
    - includes capability-domain discovery (`system__describe_capability_domain`) for deeper docs/capabilities/recipes
    - `system__get_context` returns authoritative runtime/session context snapshots
- `AgentOrchestrator` with:
  - session-scoped tool visibility sourced from engaged capability domains only
  - provider-neutral model adapter boundary
  - semantic retry strategy for recoverable invalid tool calls
  - compaction-aware prompt stats and invocation diagnostics

### CapabilityDomain Contracts
- `fathom-capability-domain`:
  - shared capability-domain/action contracts (`CapabilityDomain`, `Action`, `ActionSpec`, `ActionOutcome`)
  - capability-domain metadata includes `id`, `name`, and `description`
  - canonical naming helpers (`env__action`)
- `envs/fathom-capability-domain-fs`:
  - filesystem capability-domain action instances (`get_base_path`, `list`, `read`, `write`, `replace`, `glob`, `search`)
  - action schemas and validation
  - filesystem execution backend (path parsing, sandboxing, real I/O)
- `envs/fathom-capability-domain-brave-search`:
  - Brave Search capability-domain action instance (`web_search`)
  - action schema and validation
  - API execution backend (server-side credential auth, compact result mapping, structured provider/network failures)
- `envs/fathom-capability-domain-jina`:
  - Jina Reader capability-domain action instance (`read_url`)
  - action schema and validation
  - API execution backend (server-side credential auth, URL validation, markdown extraction, truncation metadata)
- `envs/fathom-capability-domain-shell`:
  - shell capability-domain action instance (`run`)
  - action schema and validation
  - async command execution backend (cwd/env overrides, runtime-managed timeout, bounded output capture)
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
Persistence, authorization/approval controls, and real environment backends can be layered on top of this runtime contract.

## CapabilityDomain
- Required: `OPENAI_API_KEY`
- Optional per feature: `BRAVE_API_KEY` (required when agent uses `brave_search__web_search`)
- Optional per feature: `JINA_API_KEY` (required when agent uses `jina__read_url`)
- For local development, use `direnv` or equivalent shell environment loader.
