mod error;
mod http;
mod result;

#[cfg(test)]
mod tests;

use fathom_capability_domain::ActionOutcome;
use serde::Deserialize;
use serde_json::{Value, json};

use self::error::JinaError;
use self::http::{HARD_DEFAULT_TARGET_SELECTOR, ReadRequest, ReadRequestOptions, run_reader};
use crate::validate::validate_http_url;
use crate::{JINA_ACTION_MAX_TIMEOUT_MS, JINA_MAX_CONTENT_BYTES, JINA_TOKEN_BUDGET_DEFAULT};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadUrlArgs {
    url: String,
    #[serde(default)]
    target_selector: Option<String>,
    #[serde(default)]
    remove_selector: Option<String>,
    #[serde(default)]
    wait_for_selector: Option<String>,
    #[serde(default)]
    token_budget: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

const ADVISORY: &str = "Content may be low quality. It may require retry with custom filters.";
const HARD_PROFILE: &str = "hard_default";
const SOFT_PROFILE: &str = "soft_default";

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
        Err(error) => return result::failure("read_url", &error, None, vec![], ADVISORY),
    };

    let url = args.url.trim();
    if url.is_empty() {
        return result::failure(
            "read_url",
            &JinaError::invalid_args("jina__read_url.url must be a non-empty string"),
            None,
            vec![],
            ADVISORY,
        );
    }
    if let Err(error) = validate_http_url("jina__read_url.url", url) {
        return result::failure(
            "read_url",
            &JinaError::invalid_args(error),
            None,
            vec![],
            ADVISORY,
        );
    }
    if execution_timeout_ms == 0 {
        return result::failure(
            "read_url",
            &JinaError::internal("jina reader execution timeout must be > 0"),
            None,
            vec![],
            ADVISORY,
        );
    }
    let timeout_ms = match resolve_timeout_ms(args.timeout_ms, execution_timeout_ms) {
        Ok(timeout_ms) => timeout_ms,
        Err(error) => return result::failure("read_url", &error, None, vec![], ADVISORY),
    };

    let base_options = ReadRequestOptions {
        target_selector: args.target_selector.clone(),
        remove_selector: args.remove_selector.clone(),
        wait_for_selector: args.wait_for_selector.clone(),
        token_budget: args.token_budget.unwrap_or(JINA_TOKEN_BUDGET_DEFAULT),
        retain_images_none: true,
        with_images_summary: true,
        with_links_summary: true,
    };

    let mut attempts = Vec::new();
    let hard_options = hard_profile_options(&base_options);
    let hard_request = ReadRequest {
        source_url: url.to_string(),
        timeout_ms,
        max_content_bytes: JINA_MAX_CONTENT_BYTES,
        options: hard_options.clone(),
    };
    match run_reader(hard_request).await {
        Ok(data) => {
            attempts.push(success_attempt(HARD_PROFILE, &hard_options, &data));
            return result::success("read_url", data, attempts, 0, false, ADVISORY);
        }
        Err(error) => {
            attempts.push(failed_attempt(HARD_PROFILE, &hard_options, &error));
            if !should_fallback_to_soft(&error) {
                let request_info = json!({
                    "source_url": url,
                    "max_content_bytes": JINA_MAX_CONTENT_BYTES,
                });
                return result::failure("read_url", &error, Some(request_info), attempts, ADVISORY);
            }
        }
    }

    let soft_options = soft_profile_options(&base_options);
    let soft_request = ReadRequest {
        source_url: url.to_string(),
        timeout_ms,
        max_content_bytes: JINA_MAX_CONTENT_BYTES,
        options: soft_options.clone(),
    };
    match run_reader(soft_request).await {
        Ok(data) => {
            attempts.push(success_attempt(SOFT_PROFILE, &soft_options, &data));
            result::success("read_url", data, attempts, 1, true, ADVISORY)
        }
        Err(error) => {
            attempts.push(failed_attempt(SOFT_PROFILE, &soft_options, &error));
            let request_info = json!({
                "source_url": url,
                "max_content_bytes": JINA_MAX_CONTENT_BYTES,
            });
            result::failure("read_url", &error, Some(request_info), attempts, ADVISORY)
        }
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

fn hard_profile_options(base: &ReadRequestOptions) -> ReadRequestOptions {
    let mut options = base.clone();
    if options.target_selector.is_none() {
        options.target_selector = Some(HARD_DEFAULT_TARGET_SELECTOR.to_string());
    }
    options
}

fn soft_profile_options(base: &ReadRequestOptions) -> ReadRequestOptions {
    let mut options = base.clone();
    options.target_selector = None;
    options
}

fn should_fallback_to_soft(error: &JinaError) -> bool {
    matches!(
        error.code(),
        "provider_http" | "provider_parse" | "network" | "timeout"
    )
}

fn resolve_timeout_ms(
    timeout_ms: Option<u64>,
    execution_timeout_ms: u64,
) -> Result<u64, JinaError> {
    match timeout_ms {
        None => Ok(execution_timeout_ms),
        Some(0) => Err(JinaError::invalid_args(
            "jina__read_url.timeout_ms must be a positive integer",
        )),
        Some(value) if value > JINA_ACTION_MAX_TIMEOUT_MS => Err(JinaError::invalid_args(format!(
            "jina__read_url.timeout_ms must be <= {JINA_ACTION_MAX_TIMEOUT_MS}"
        ))),
        Some(value) if value > execution_timeout_ms => Err(JinaError::invalid_args(format!(
            "jina__read_url.timeout_ms ({value}) exceeds runtime timeout ({execution_timeout_ms})"
        ))),
        Some(value) => Ok(value),
    }
}

fn success_attempt(profile: &str, options: &ReadRequestOptions, data: &Value) -> Value {
    json!({
        "profile": profile,
        "succeeded": true,
        "effective_headers": options.headers_json(),
        "provider_code": data.get("provider_code").cloned().unwrap_or(Value::Null),
        "provider_status": data.get("provider_status").cloned().unwrap_or(Value::Null),
        "warning": ADVISORY,
    })
}

fn failed_attempt(profile: &str, options: &ReadRequestOptions, error: &JinaError) -> Value {
    json!({
        "profile": profile,
        "succeeded": false,
        "effective_headers": options.headers_json(),
        "error": {
            "code": error.code(),
            "message": error.message(),
            "details": error.details().cloned().unwrap_or(Value::Null),
        },
    })
}
