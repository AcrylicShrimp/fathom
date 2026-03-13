mod error;
mod http;
mod result;

use fathom_capability_domain::CapabilityActionResult;
use serde::Deserialize;
use serde_json::{Value, json};

use self::error::BraveError;
use self::http::{WebSearchRequest, run_web_search};
use crate::{BRAVE_SEARCH_DEFAULT_COUNT, BRAVE_SEARCH_DEFAULT_SAFESEARCH, BRAVE_SEARCH_MAX_COUNT};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebSearchArgs {
    query: String,
    count: Option<u8>,
}

pub async fn execute_action(
    action_name: &str,
    args_json: &str,
    _environment_state: &Value,
    execution_timeout_ms: u64,
) -> Option<CapabilityActionResult> {
    match action_name {
        "web_search" => Some(execute_web_search(args_json, execution_timeout_ms).await),
        _ => None,
    }
}

async fn execute_web_search(args_json: &str, execution_timeout_ms: u64) -> CapabilityActionResult {
    let args = match parse_args::<WebSearchArgs>(args_json, "brave_search__web_search") {
        Ok(args) => args,
        Err(error) => return result::failure("web_search", &error, None),
    };

    let query = args.query.trim();
    if query.is_empty() {
        let error =
            BraveError::invalid_args("brave_search__web_search.query must be a non-empty string");
        return result::failure("web_search", &error, None);
    }

    if execution_timeout_ms == 0 {
        let error = BraveError::internal("brave_search execution timeout must be > 0");
        return result::failure("web_search", &error, None);
    }

    let count = args
        .count
        .unwrap_or(BRAVE_SEARCH_DEFAULT_COUNT)
        .clamp(1, BRAVE_SEARCH_MAX_COUNT);
    let request = WebSearchRequest {
        query: query.to_string(),
        count,
        safesearch: BRAVE_SEARCH_DEFAULT_SAFESEARCH.to_string(),
        timeout_ms: execution_timeout_ms,
    };

    let request_info = json!({
        "query": request.query,
        "count": request.count,
        "safesearch": request.safesearch,
    });

    match run_web_search(request).await {
        Ok(data) => result::success("web_search", data),
        Err(error) => result::failure("web_search", &error, Some(request_info)),
    }
}

fn parse_args<T: for<'de> Deserialize<'de>>(
    args_json: &str,
    action_id: &str,
) -> Result<T, BraveError> {
    serde_json::from_str(args_json).map_err(|error| {
        BraveError::invalid_args(format!("{action_id} arguments are invalid: {error}"))
    })
}
