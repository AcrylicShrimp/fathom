use std::time::Duration;

use serde_json::{Value, json};

use super::error::JinaError;

const JINA_READER_URL: &str = "https://r.jina.ai/";
const JINA_API_KEY_ENV: &str = "JINA_API_KEY";
const ERROR_BODY_PREVIEW_BYTES: usize = 2_048;

#[derive(Debug, Clone)]
pub(crate) struct ReadRequest {
    pub(crate) source_url: String,
    pub(crate) timeout_ms: u64,
    pub(crate) max_content_bytes: usize,
}

pub(crate) async fn run_reader(request: ReadRequest) -> Result<Value, JinaError> {
    let api_key = std::env::var(JINA_API_KEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            JinaError::auth_missing(format!("{JINA_API_KEY_ENV} is required for jina__read_url"))
        })?;

    let timeout = Duration::from_millis(request.timeout_ms);
    let client = reqwest::Client::new();
    let response = client
        .post(JINA_READER_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .header("X-Return-Format", "markdown")
        .timeout(timeout)
        .json(&json!({
            "url": request.source_url,
        }))
        .send()
        .await
        .map_err(map_transport_error)?;

    let status = response.status();
    let body = response.text().await.map_err(map_transport_error)?;

    if !status.is_success() {
        return Err(JinaError::provider_http(format!(
            "Jina reader request failed with status {status}"
        ))
        .with_details(json!({
            "status_code": status.as_u16(),
            "response_body_preview": preview_text(&body, ERROR_BODY_PREVIEW_BYTES),
        })));
    }

    let response_json: Value = serde_json::from_str(&body).map_err(|error| {
        JinaError::provider_parse(format!(
            "failed to parse Jina reader JSON response: {error}"
        ))
        .with_details(json!({
            "response_body_preview": preview_text(&body, ERROR_BODY_PREVIEW_BYTES),
        }))
    })?;

    let extracted = extract_reader_payload(&response_json, request.source_url.as_str())?;
    let (content_markdown, truncated_bytes) = truncate_utf8_by_bytes(
        extracted.content_markdown.as_str(),
        request.max_content_bytes,
    );

    let mut output = json!({
        "source_url": request.source_url,
        "resolved_url": extracted.resolved_url,
        "content_markdown": content_markdown,
        "content_bytes": content_markdown.len(),
        "original_content_bytes": extracted.content_markdown.len(),
        "truncated": truncated_bytes > 0,
        "truncated_bytes": truncated_bytes,
        "max_content_bytes": request.max_content_bytes,
    });

    if let Some(title) = extracted.title {
        output["title"] = json!(title);
    }
    if let Some(description) = extracted.description {
        output["description"] = json!(description);
    }
    if let Some(provider_code) = extracted.provider_code {
        output["provider_code"] = json!(provider_code);
    }
    if let Some(provider_status) = extracted.provider_status {
        output["provider_status"] = json!(provider_status);
    }

    Ok(output)
}

#[derive(Debug, Clone)]
struct ExtractedPayload {
    resolved_url: String,
    title: Option<String>,
    description: Option<String>,
    content_markdown: String,
    provider_code: Option<i64>,
    provider_status: Option<i64>,
}

fn extract_reader_payload(
    response_json: &Value,
    source_url: &str,
) -> Result<ExtractedPayload, JinaError> {
    let data = response_json.get("data").and_then(Value::as_object);

    let content_markdown = data
        .and_then(|map| map.get("content"))
        .and_then(Value::as_str)
        .or_else(|| response_json.get("content").and_then(Value::as_str))
        .ok_or_else(|| {
            JinaError::provider_parse("missing `data.content` string in Jina reader response")
        })?;

    let title = data
        .and_then(|map| map.get("title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            response_json
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        });

    let description = data
        .and_then(|map| map.get("description"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            response_json
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        });

    let resolved_url = data
        .and_then(|map| map.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(source_url)
        .to_string();

    Ok(ExtractedPayload {
        resolved_url,
        title,
        description,
        content_markdown: content_markdown.to_string(),
        provider_code: response_json.get("code").and_then(Value::as_i64),
        provider_status: response_json.get("status").and_then(Value::as_i64),
    })
}

fn map_transport_error(error: reqwest::Error) -> JinaError {
    if error.is_timeout() {
        JinaError::timeout(format!("jina reader request timed out: {error}"))
    } else {
        JinaError::network(format!("jina reader request failed: {error}"))
    }
}

fn truncate_utf8_by_bytes(value: &str, max_bytes: usize) -> (String, usize) {
    if value.len() <= max_bytes {
        return (value.to_string(), 0);
    }

    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
        return (String::new(), value.len());
    }

    let truncated_bytes = value.len().saturating_sub(end);
    (value[..end].to_string(), truncated_bytes)
}

fn preview_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{extract_reader_payload, preview_text, truncate_utf8_by_bytes};

    #[test]
    fn extract_payload_from_data_content() {
        let payload = json!({
            "code": 200,
            "status": 20000,
            "data": {
                "title": "Hello",
                "description": "Desc",
                "url": "https://example.com/page",
                "content": "# Heading\nBody"
            }
        });

        let extracted =
            extract_reader_payload(&payload, "https://source.com").expect("payload should parse");
        assert_eq!(extracted.resolved_url, "https://example.com/page");
        assert_eq!(extracted.title.as_deref(), Some("Hello"));
        assert_eq!(extracted.description.as_deref(), Some("Desc"));
        assert_eq!(extracted.content_markdown, "# Heading\nBody");
        assert_eq!(extracted.provider_code, Some(200));
        assert_eq!(extracted.provider_status, Some(20000));
    }

    #[test]
    fn extract_payload_requires_content_field() {
        let payload = json!({
            "data": {
                "title": "No Content"
            }
        });
        assert!(extract_reader_payload(&payload, "https://source.com").is_err());
    }

    #[test]
    fn truncate_utf8_preserves_char_boundaries() {
        let value = "가나다라마바사";
        let (truncated, omitted) = truncate_utf8_by_bytes(value, 7);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(omitted > 0);
    }

    #[test]
    fn truncate_utf8_returns_original_when_under_limit() {
        let value = "abc";
        let (truncated, omitted) = truncate_utf8_by_bytes(value, 100);
        assert_eq!(truncated, "abc");
        assert_eq!(omitted, 0);
    }

    #[test]
    fn preview_text_truncates_safely() {
        let value = "áéíóú-abcdef";
        let preview = preview_text(value, 7);
        assert!(preview.ends_with('…'));
    }
}
