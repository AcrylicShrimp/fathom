use std::time::Duration;

use serde_json::{Value, json};

use super::error::BraveError;

const BRAVE_WEB_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const BRAVE_API_KEY_ENV: &str = "BRAVE_API_KEY";
const ERROR_BODY_PREVIEW_BYTES: usize = 2_048;

#[derive(Debug, Clone)]
pub(crate) struct WebSearchRequest {
    pub(crate) query: String,
    pub(crate) count: u8,
    pub(crate) safesearch: String,
    pub(crate) timeout_ms: u64,
}

pub(crate) async fn run_web_search(request: WebSearchRequest) -> Result<Value, BraveError> {
    let api_key = std::env::var(BRAVE_API_KEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BraveError::auth_missing(format!(
                "{BRAVE_API_KEY_ENV} is required for brave_search__web_search"
            ))
        })?;

    let timeout = Duration::from_millis(request.timeout_ms);
    let client = reqwest::Client::new();
    let mut url = reqwest::Url::parse(BRAVE_WEB_SEARCH_URL)
        .map_err(|error| BraveError::internal(format!("invalid Brave endpoint URL: {error}")))?;
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("q", request.query.as_str());
        query_pairs.append_pair("count", &request.count.to_string());
        query_pairs.append_pair("safesearch", request.safesearch.as_str());
    }
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("X-Subscription-Token", api_key)
        .timeout(timeout)
        .send()
        .await
        .map_err(map_transport_error)?;

    let status = response.status();
    let body = response.text().await.map_err(map_transport_error)?;

    if !status.is_success() {
        return Err(BraveError::provider_http(format!(
            "Brave search request failed with status {status}"
        ))
        .with_details(json!({
            "status_code": status.as_u16(),
            "response_body_preview": preview_text(&body, ERROR_BODY_PREVIEW_BYTES),
        })));
    }

    let response_json: Value = serde_json::from_str(&body).map_err(|error| {
        BraveError::provider_parse(format!(
            "failed to parse Brave search response JSON: {error}"
        ))
        .with_details(json!({
            "response_body_preview": preview_text(&body, ERROR_BODY_PREVIEW_BYTES),
        }))
    })?;

    let results = extract_results(&response_json);
    Ok(json!({
        "query": request.query,
        "count": request.count,
        "safesearch": request.safesearch,
        "result_count": results.len(),
        "results": results,
    }))
}

fn map_transport_error(error: reqwest::Error) -> BraveError {
    if error.is_timeout() {
        BraveError::timeout(format!("brave search request timed out: {error}"))
    } else {
        BraveError::network(format!("brave search request failed: {error}"))
    }
}

fn extract_results(response_json: &Value) -> Vec<Value> {
    let Some(items) = response_json
        .get("web")
        .and_then(|value| value.get("results"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| normalize_result(index + 1, item))
        .collect()
}

fn normalize_result(rank: usize, item: &Value) -> Option<Value> {
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let url = item.get("url").and_then(Value::as_str).unwrap_or("").trim();
    let description = item
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    if title.is_empty() || url.is_empty() {
        return None;
    }

    let mut normalized = json!({
        "rank": rank,
        "title": title,
        "url": url,
        "description": description,
    });

    if let Some(age) = item.get("age").and_then(Value::as_str) {
        normalized["age"] = json!(age);
    }
    if let Some(language) = item.get("language").and_then(Value::as_str) {
        normalized["language"] = json!(language);
    }
    if let Some(page_age) = item.get("page_age").and_then(Value::as_str) {
        normalized["page_age"] = json!(page_age);
    }
    if let Some(source) = item.get("source").and_then(Value::as_str) {
        normalized["source"] = json!(source);
    }

    Some(normalized)
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
    use serde_json::{Value, json};

    use super::{extract_results, preview_text};

    #[test]
    fn extract_results_maps_ranked_entries() {
        let payload = json!({
            "web": {
                "results": [
                    {
                        "title": "Result A",
                        "url": "https://example.com/a",
                        "description": "alpha",
                        "age": "1 day"
                    },
                    {
                        "title": "Result B",
                        "url": "https://example.com/b",
                        "description": "beta"
                    }
                ]
            }
        });

        let results = extract_results(&payload);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["rank"], json!(1));
        assert_eq!(results[0]["title"], json!("Result A"));
        assert_eq!(results[0]["url"], json!("https://example.com/a"));
        assert_eq!(results[0]["age"], json!("1 day"));
        assert_eq!(results[1]["rank"], json!(2));
        assert_eq!(results[1]["title"], json!("Result B"));
    }

    #[test]
    fn extract_results_skips_items_missing_title_or_url() {
        let payload = json!({
            "web": {
                "results": [
                    {
                        "title": "",
                        "url": "https://example.com/a",
                        "description": "missing title"
                    },
                    {
                        "title": "Has title",
                        "url": "",
                        "description": "missing url"
                    },
                    {
                        "title": "Valid",
                        "url": "https://example.com/ok",
                        "description": "ok"
                    }
                ]
            }
        });

        let results = extract_results(&payload);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], json!("Valid"));
        assert_eq!(results[0]["url"], json!("https://example.com/ok"));
    }

    #[test]
    fn preview_text_truncates_multibyte_safely() {
        let value = "áéíóú-abcdef";
        let preview = preview_text(value, 7);
        assert!(preview.ends_with('…'));
        assert!(preview.len() < value.len() + 3);
    }

    #[test]
    fn preview_text_keeps_short_text() {
        let value = "short";
        assert_eq!(preview_text(value, 100), value.to_string());
    }

    #[test]
    fn extract_results_returns_empty_when_missing_web_results() {
        let payload = json!({
            "web": {
                "results": Value::Null
            }
        });
        assert!(extract_results(&payload).is_empty());
    }
}
