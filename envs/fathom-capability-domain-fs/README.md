# fathom-capability-domain-fs

Filesystem capability-domain implementation for Fathom's `env + action` runtime model.

This crate provides the canonical `filesystem` capability domain and executes these actions:

- `filesystem__get_base_path`
- `filesystem__list`
- `filesystem__read`
- `filesystem__write`
- `filesystem__replace`
- `filesystem__glob`
- `filesystem__search`

## Purpose

`fathom-capability-domain-fs` gives agents safe, non-destructive file operations rooted at a configured base path.

- Scope is constrained to `base_path`.
- Paths are normalized and validated as relative paths.
- No delete/move actions are exposed.
- Text operations (`read`, `replace`, `search`) are UTF-8 only.

## CapabilityDomain Model

CapabilityDomain ID: `filesystem`

Default initial state:

```json
{
  "base_path": "."
}
```

`base_path` may be absolute or relative in state. At runtime it is canonicalized to an absolute directory.

## Path Policy

All path-bearing actions enforce:

- non-empty string
- relative path only
- no URI scheme (`://`)
- no escape above `base_path`

Use `.` to reference the capability-domain root.

## Response Envelope

Every action returns an `ActionOutcome.message` JSON string with this envelope shape.

Success:

```json
{
  "ok": true,
  "op": "read",
  "path": "src/main.rs",
  "target": "filesystem",
  "data": {}
}
```

Failure:

```json
{
  "ok": false,
  "op": "read",
  "path": "src/main.rs",
  "target": "filesystem",
  "error_code": "not_found",
  "message": "No such file or directory (os error 2)"
}
```

## Error Codes

- `invalid_args`
- `invalid_path`
- `invalid_encoding`
- `not_found`
- `not_file`
- `not_directory`
- `already_exists`
- `permission_denied`
- `io_error`

`invalid_encoding` is returned when `read`, `replace`, or `search` touches a non-UTF-8 file.

## Action Reference

### `filesystem__get_base_path`

Return the canonical current base path.

Request:

```json
{}
```

Response `data`:

```json
{
  "base_path": "/absolute/path",
  "source": "filesystem_env_state"
}
```

---

### `filesystem__list`

List entries under a relative directory.

Request schema:

```json
{
  "path": "string",
  "recursive": "boolean (optional, default false)",
  "max_entries": "integer >= 1 (optional, default 200, cap 5000)",
  "include_hidden": "boolean (optional, default false)"
}
```

Response `data`:

```json
{
  "entries": [
    { "path": "src", "name": "src", "kind": "dir" },
    { "path": "src/main.rs", "name": "main.rs", "kind": "file", "size": 1234 }
  ],
  "truncated": false,
  "next_cursor": null
}
```

Notes:

- `kind` is `dir`, `file`, or `other`.
- Hidden filtering is name-based at each traversal step.

---

### `filesystem__read`

Read UTF-8 text by line window.

Request schema:

```json
{
  "path": "string",
  "offset_line": "integer >= 1 (optional, default 1)",
  "limit_lines": "integer >= 1 (optional, default 200, cap 2000)"
}
```

Response `data`:

```json
{
  "content": "line1\nline2",
  "start_line": 1,
  "returned_lines": 2,
  "total_lines": 18,
  "truncated": true,
  "bytes": 2048
}
```

Notes:

- Line splitting uses Rust `str::lines()` semantics.
- If `offset_line` is past EOF, `content` is empty and `returned_lines` is `0`.

---

### `filesystem__write`

Write text content to a file.

Request schema:

```json
{
  "path": "string",
  "content": "string",
  "allow_override": "boolean (required)",
  "create_parents": "boolean (optional, default true)"
}
```

Response `data`:

```json
{
  "bytes_written": 17,
  "created": true,
  "overwritten": false
}
```

Notes:

- If target exists and `allow_override=false`, returns `already_exists`.
- If parent directory is missing and `create_parents=false`, returns `not_found`.

---

### `filesystem__replace`

Replace literal text in a UTF-8 file.

Request schema:

```json
{
  "path": "string",
  "old": "non-empty string",
  "new": "string",
  "mode": "first | all",
  "expected_replacements": "integer >= 0 (optional)"
}
```

Response `data`:

```json
{
  "replacements": 1,
  "bytes": 1512
}
```

Notes:

- `mode=first` replaces at most one occurrence.
- `mode=all` replaces all occurrences.
- If `expected_replacements` is set and does not match actual count, returns `invalid_args`.

---

### `filesystem__glob`

Find files by glob pattern.

Request schema:

```json
{
  "pattern": "non-empty glob string (required)",
  "path": "string (optional, default '.')",
  "max_results": "integer >= 1 (optional, default 500, cap 5000)",
  "include_hidden": "boolean (optional, default false)"
}
```

Response `data`:

```json
{
  "matches": ["src/main.rs", "src/lib.rs"],
  "truncated": false
}
```

Notes:

- Traversal yields files only, not directories.
- For directory scopes, matching is attempted against both:
  - path relative to selected `path`
  - path relative to environment `base_path`
- Hidden filtering applies only when traversing directories.

---

### `filesystem__search`

Regex search over UTF-8 files.

Request schema:

```json
{
  "pattern": "non-empty regex string (required)",
  "path": "string (optional, default '.')",
  "include": ["glob1", "glob2"],
  "max_results": "integer >= 1 (optional, default 200, cap 10000)",
  "case_sensitive": "boolean (optional, default false)"
}
```

Response `data`:

```json
{
  "matches": [
    {
      "path": "src/main.rs",
      "line": 12,
      "column": 4,
      "preview": "fn main() {"
    }
  ],
  "truncated": false
}
```

Notes:

- Regex engine is Rust `regex` crate syntax.
- Invalid regex returns `invalid_args`.
- When scanning directories, hidden files/directories are skipped.
- If any scanned file is non-UTF-8, the action fails with `invalid_encoding`.

## Non-Destructive Scope

This env intentionally does not include delete/rename/move actions.
If destructive operations are needed, expose them via a separate environment (for example, shell/runtime env).

## Local Development

From workspace root:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test -p fathom-capability-domain-fs
```
