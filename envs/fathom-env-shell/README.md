# fathom-env-shell

Shell environment implementation for Fathom's `env + action` runtime model.

This crate provides the canonical `shell` environment and executes:

- `shell__run`

## Purpose

Provide a single non-interactive command execution action for agents.

- Commands run under a base path (`base_path`) defined by environment state.
- Working directory is `base_path` joined with optional relative `path`.
- stdout/stderr are captured with byte caps and truncation metadata.
- Timeouts are enforced per call.
- Non-zero exit code marks task/action failure.

## Environment

Environment ID: `shell`

Initial state:

```json
{
  "base_path": "."
}
```

In server sessions, `base_path` is set to the runtime workspace root.

## Path Policy

`path` values must be:

- non-empty relative filesystem paths
- without URI schemes (no `://`)
- within `base_path` (no escape like `../../..`)
- directories (for command working directory)

Use `.` for the environment root directory.

## Action Reference

### `shell__run`

Execute one shell command in non-interactive mode.

Input:

```json
{
  "command": "string (required)",
  "path": "string (optional, default '.')",
  "env": {
    "KEY": "value"
  },
  "timeout_ms": "integer (optional, default 30000, max 300000)"
}
```

Validation notes:

- `command` must be non-empty and <= 16384 bytes.
- `env` keys must match `[A-Za-z_][A-Za-z0-9_]*`.
- max env entries: 128.

Success payload envelope (`ActionOutcome.message` JSON string):

```json
{
  "ok": true,
  "op": "run",
  "path": ".",
  "target": "shell",
  "data": {
    "command": "printf 'hello'",
    "effective_cwd": "/abs/path",
    "exit_code": 0,
    "stdout": "hello",
    "stderr": "",
    "stdout_truncated_bytes": 0,
    "stderr_truncated_bytes": 0,
    "duration_ms": 4,
    "timed_out": false
  }
}
```

Failure payload envelope:

```json
{
  "ok": false,
  "op": "run",
  "path": ".",
  "target": "shell",
  "error": {
    "code": "execution_failed",
    "message": "command exited with non-zero status 7"
  },
  "data": {
    "command": "echo fail; exit 7",
    "effective_cwd": "/abs/path",
    "exit_code": 7,
    "stdout": "",
    "stderr": "fail\n",
    "stdout_truncated_bytes": 0,
    "stderr_truncated_bytes": 0,
    "duration_ms": 3,
    "timed_out": false
  }
}
```

## Error Codes

The `error.code` field can be:

- `invalid_args`
- `invalid_path`
- `not_found`
- `not_directory`
- `permission_denied`
- `io_error`
- `spawn_failed`
- `timeout`
- `execution_failed`
- `internal`

## Runtime Notes

- Unix uses `/bin/sh -lc <command>`.
- Windows uses `cmd /C <command>`.
- Output caps:
  - stdout: 65536 bytes
  - stderr: 65536 bytes
- Output decoding is UTF-8 lossy to avoid hard failures on arbitrary bytes.
- No approval/sandbox policy is enforced by this crate (current runtime decision).
