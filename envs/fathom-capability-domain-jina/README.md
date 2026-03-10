# fathom-capability-domain-jina

Jina Reader capability-domain implementation for Fathom's `env + action` runtime model.

This crate provides one action:

- `jina__read_url`

## Purpose

Provide readable webpage extraction for one URL at a time.

- Server-side auth via `JINA_API_KEY`.
- Input URL must be absolute `http(s)`.
- Output body is extracted content plus metadata.
- Large payloads are hard-truncated with explicit truncation fields.
- Runtime uses two-stage defaults:
  - `hard_default`: selector-first attempt
  - `soft_default`: no-selector fallback (when hard attempt fails with provider/transport errors)

## CapabilityDomain

CapabilityDomain ID: `jina`

Initial state:

```json
{}
```

## Action Reference

### `jina__read_url`

Read one URL via Jina Reader API and return extracted content.

Input:

```json
{
  "url": "string (required, absolute http/https)",
  "target_selector": "string (optional)",
  "remove_selector": "string (optional)",
  "wait_for_selector": "string (optional)",
  "token_budget": "integer (optional, default 200000)",
  "timeout_ms": "integer (optional)"
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
  },
  "attempts": [
    {
      "profile": "hard_default",
      "succeeded": true,
      "effective_headers": {
        "X-Retain-Images": "none",
        "X-With-Images-Summary": "true",
        "X-With-Links-Summary": "true",
        "X-Token-Budget": "200000",
        "X-Target-Selector": "main, section, article"
      },
      "provider_code": 200,
      "provider_status": 20000,
      "warning": "Content may be low quality. It may require retry with custom filters."
    }
  ],
  "selected_attempt_index": 0,
  "used_fallback": false,
  "advisory": "Content may be low quality. It may require retry with custom filters."
}
```

Failure payload envelope:

```json
{
  "ok": false,
  "op": "read_url",
  "target": "jina",
  "attempts": [
    {
      "profile": "hard_default",
      "succeeded": false
    },
    {
      "profile": "soft_default",
      "succeeded": false
    }
  ],
  "advisory": "Content may be low quality. It may require retry with custom filters.",
  "error": {
    "code": "provider_http",
    "message": "Jina reader provider error: ..."
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
- Default request headers:
  - `X-Retain-Images: none`
  - `X-With-Images-Summary: true`
  - `X-With-Links-Summary: true`
  - `X-Token-Budget: 200000`
- No explicit `X-Engine` or `X-Return-Format` header is set by default.
- Timeout is runtime-managed via action timeout policy (default 30000ms, max 30000ms).
