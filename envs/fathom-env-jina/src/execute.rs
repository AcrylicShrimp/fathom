mod error;
mod http;
mod result;

#[cfg(test)]
mod tests;

use fathom_env::ActionOutcome;
use serde::Deserialize;
use serde_json::{Value, json};

use self::error::JinaError;
use self::http::{ReadRequest, run_reader};
use crate::JINA_MAX_CONTENT_BYTES;
use crate::validate::validate_http_url;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadUrlArgs {
    url: String,
}

pub async fn execute_action(
    action_name: &str,
    args_json: &str,
    _environment_state: &Value,
    execution_timeout_ms: u64,
) -> Option<ActionOutcome> {
    match action_name {
        "read_url" => Some(execute_read_url(args_json, execution_timeout_ms).await),
        _ => None,
    }
}

async fn execute_read_url(args_json: &str, execution_timeout_ms: u64) -> ActionOutcome {
    let args = match parse_args::<ReadUrlArgs>(args_json, "jina__read_url") {
        Ok(args) => args,
        Err(error) => return result::failure("read_url", &error, None),
    };

    let url = args.url.trim();
    if url.is_empty() {
        return result::failure(
            "read_url",
            &JinaError::invalid_args("jina__read_url.url must be a non-empty string"),
            None,
        );
    }
    if let Err(error) = validate_http_url("jina__read_url.url", url) {
        return result::failure("read_url", &JinaError::invalid_args(error), None);
    }
    if execution_timeout_ms == 0 {
        return result::failure(
            "read_url",
            &JinaError::internal("jina reader execution timeout must be > 0"),
            None,
        );
    }

    let request = ReadRequest {
        source_url: url.to_string(),
        timeout_ms: execution_timeout_ms,
        max_content_bytes: JINA_MAX_CONTENT_BYTES,
    };
    let request_info = json!({
        "source_url": request.source_url,
        "max_content_bytes": request.max_content_bytes,
    });

    match run_reader(request).await {
        Ok(data) => result::success("read_url", data),
        Err(error) => result::failure("read_url", &error, Some(request_info)),
    }
}

fn parse_args<T: for<'de> Deserialize<'de>>(
    args_json: &str,
    action_id: &str,
) -> Result<T, JinaError> {
    serde_json::from_str(args_json).map_err(|error| {
        JinaError::invalid_args(format!("{action_id} arguments are invalid: {error}"))
    })
}
