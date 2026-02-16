# Fathom Architecture

## Overview
Fathom is a session-oriented agent runtime with a gRPC server and TUI client.

- Server manages many sessions concurrently.
- Each session is processed by a single actor task for deterministic ordering.
- Clients can attach to one or more sessions and consume event streams.
- Agent input is trigger-based, not direct message-based.
- Agent intelligence is backed by OpenAI Responses API.
- Assistant user-facing output supports streaming from server to client.

## Core Concepts

### Session
A session is the unit of conversation and orchestration.

- Accepts triggers from users, tasks, heartbeat, and cron.
- Runs exactly one agent turn at a time.
- Maintains:
  - immutable profile copies (`agent_profile_copy`, `participant_user_profiles_copy`)
  - queued triggers
  - conversation history
  - task registry and task scheduler state

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
3. Invoke OpenAI Responses API in streaming mode with tool-only policy.
4. Stream tool-argument deltas (notably `send_message`) into session events.
5. Dispatch tool calls immediately as background tasks.
6. Emit session events.
7. Flush trigger snapshot and assistant outputs into history atomically.

### Task
Tasks are background jobs created by agent actions.

- States: `Pending`, `Running`, `Succeeded`, `Failed`, `Canceled`.
- Scheduling policy:
  - Start immediately when worker capacity is available.
  - Otherwise remain `Pending`.
- Task completion re-enters the session as `Trigger::TaskDone`.
- One model tool call maps to one background task.
- Implemented filesystem tools execute as real background jobs:
  - `fs_list(path)`
  - `fs_read(path)`
  - `fs_write(path, content, allow_override)`
  - `fs_replace(path, old, new, mode)`
- Trigger policy:
  - General tools enqueue `TaskDone` triggers and can drive a follow-up agent turn.
  - `send_message(content, user_id?)` is a system tool that emits user-facing assistant output but does **not** enqueue `TaskDone` (prevents self-loop chains).
- Streaming policy for `send_message`:
  - Server consumes OpenAI function-argument deltas for `send_message`.
  - Server extracts incremental `content` text and emits `AssistantStream` events.
  - Deltas are coalesced with a short batch window (~40ms) to reduce event volume.
  - Final `AssistantOutput` includes `stream_id` to correlate and deduplicate client-side live streams.
- History transformation policy:
  - `task_started` and `task_finished` are recorded as distinct history events.
  - Task args/results are stored in history as truncated previews with byte/line cutoff metadata and lookup references.
  - Agent can query full payloads with `sys_get_task_payload`.

### Filesystem Path Spaces
Filesystem tools use URI-style paths:

- `managed://...` for profile-backed managed files
  - `managed://agent/<agent_id>/<field>`
  - `managed://user/<user_id>/<field>`
- `fs://...` for real workspace files (workspace-relative only)

Managed files are mapped to profile fields (agent/user profile content and memory). Real filesystem paths are sandboxed to the configured workspace root.

## Identity and Memory

Profiles are canonical global entities:

- `AgentProfile`
  - includes managed content fields for `AGENTS.md`, `SOUL.md`, `IDENTITY.md`
  - includes long-term agent memory
- `UserProfile`
  - includes managed content field for `USER.md`
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
  - `agent/*`: model orchestration, prompt rendering, tool schema
  - `session/*`: deterministic turn actor + task policy
  - `session/engine/assistant_stream.rs`: `send_message` streaming extraction and batching
  - `history/*`: structured history line transformation and preview truncation
  - `system_tools/*`: runtime/profile/session/task discovery tools
  - `fs/*`: managed and real filesystem tools
- OpenAI-backed `AgentOrchestrator` with:
  - static tool registry (server-defined tools only)
  - streaming Responses API integration
  - retry policy with backoff/jitter and `Retry-After` support

### Client (`fathom-client`)
- gRPC client wrapper for runtime API.
- TUI runtime that:
  - creates/upserts profiles
  - creates a session
  - subscribes to session events
  - enqueues user and heartbeat triggers
  - transforms all inbound events into one canonical internal `EventRecord`
  - routes the same `EventRecord` stream to all tab implementations
- Tab architecture:
  - `Conversation` tab:
    - chat-oriented projection only (user + assistant conversation lines)
    - user lines are derived from local send actions
    - assistant lines are rendered inline and updated smoothly during streaming
    - internal/system diagnostics are excluded from this tab
  - `Events` tab: full-fidelity debug event stream
  - tab switching via `Shift+Tab`

### CLI (`fathom`)
- `fathom server --addr ...`
- `fathom client --server ...`
- `cargo run` starts server + client in a combined local flow

## Current Scope
This implementation is intentionally in-memory and bootstrap-focused.
Persistence, authorization/approval policy, and real tool backends can be layered on top of this runtime contract.

## Environment
- Required: `OPENAI_API_KEY`
- For local development, use `direnv` or equivalent shell environment loader.
