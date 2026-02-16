# fathom-env-brave-search

Brave Search environment implementation for Fathom's `env + action` runtime model.

This crate provides one action:

- `brave_search__web_search`

## Purpose

Provide web search capability to agents using Brave Search API, with compact result metadata suitable for prompt context.

- Server-side auth via `BRAVE_API_KEY`.
- Query-based search only (no URL fetch in v1).
- Structured success/failure payload envelopes.
- Runtime-managed timeout policy via `ActionSpec`.

## Environment

Environment ID: `brave_search`

Initial state:

```json
{}
```

## Action Reference

### `brave_search__web_search`

Search the web using Brave Search API.

Input:

```json
{
  "query": "string (required, non-empty)",
  "count": "integer (optional, 1..20, default 5)"
}
```

Success payload envelope (`ActionOutcome.message` JSON string):

```json
{
  "ok": true,
  "op": "web_search",
  "target": "brave_search",
  "data": {
    "query": "rust tonic grpc",
    "count": 5,
    "safesearch": "off",
    "result_count": 2,
    "results": [
      {
        "rank": 1,
        "title": "Tonic",
        "url": "https://github.com/hyperium/tonic",
        "description": "A native gRPC client & server implementation for Rust."
      }
    ]
  }
}
```

Failure payload envelope:

```json
{
  "ok": false,
  "op": "web_search",
  "target": "brave_search",
  "error": {
    "code": "auth_missing",
    "message": "BRAVE_API_KEY is required for brave_search__web_search"
  },
  "data": {
    "query": "rust tonic grpc",
    "count": 5,
    "safesearch": "off"
  }
}
```

## Error Codes

The `error.code` field can be:

- `invalid_args`
- `auth_missing`
- `provider_http`
- `provider_parse`
- `network`
- `timeout`
- `internal`

## Notes

- `safesearch` default is `off` in v1.
- Output is intentionally compact metadata, not full-page content.
- For richer page extraction, a separate environment can be added later.
