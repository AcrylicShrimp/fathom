# fathom-env-jina

Jina Reader environment implementation for Fathom's `env + action` runtime model.

This crate provides one action:

- `jina__read_url`

## Purpose

Provide readable webpage extraction for one URL at a time.

- Server-side auth via `JINA_API_KEY`.
- Input URL must be absolute `http(s)`.
- Output body is markdown plus metadata.
- Large payloads are hard-truncated with explicit truncation fields.

## Environment

Environment ID: `jina`

Initial state:

```json
{}
```

## Action Reference

### `jina__read_url`

Read one URL via Jina Reader API and return extracted markdown.

Input:

```json
{
  "url": "string (required, absolute http/https)"
}
```

Success payload envelope (`ActionOutcome.message` JSON string):

```json
{
  "ok": true,
  "op": "read_url",
  "target": "jina",
  "data": {
    "source_url": "https://example.com",
    "resolved_url": "https://example.com/article",
    "title": "Example Title",
    "description": "Optional description",
    "content_markdown": "# Heading\n...",
    "content_bytes": 4120,
    "original_content_bytes": 6255,
    "truncated": false,
    "truncated_bytes": 0,
    "max_content_bytes": 100000,
    "provider_code": 200,
    "provider_status": 20000
  }
}
```

Failure payload envelope:

```json
{
  "ok": false,
  "op": "read_url",
  "target": "jina",
  "error": {
    "code": "auth_missing",
    "message": "JINA_API_KEY is required for jina__read_url"
  },
  "data": {
    "source_url": "https://example.com",
    "max_content_bytes": 100000
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

## Runtime Notes

- Endpoint: `https://r.jina.ai/` with JSON body `{"url":"..."}`.
- Authorization header: `Bearer <JINA_API_KEY>`.
- Return format request header is set to markdown.
- Timeout is runtime-managed via action timeout policy.
