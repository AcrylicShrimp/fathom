use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{args_object, require_non_empty_string, validate_http_url};
use crate::{JINA_ACTION_DESIRED_TIMEOUT_MS, JINA_ACTION_MAX_TIMEOUT_MS, JINA_ENVIRONMENT_ID};

const MAX_URL_BYTES: usize = 8_192;

pub struct JinaReadUrlAction;

impl Action for JinaReadUrlAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: JINA_ENVIRONMENT_ID,
            action_name: "read_url",
            description: "Read one absolute http(s) URL via Jina Reader API and return extracted markdown content with metadata.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
            discovery: false,
            max_timeout_ms: JINA_ACTION_MAX_TIMEOUT_MS,
            desired_timeout_ms: Some(JINA_ACTION_DESIRED_TIMEOUT_MS),
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        let url = require_non_empty_string(args, "url")?;
        if url.len() > MAX_URL_BYTES {
            return Err(format!(
                "jina__read_url.url must be <= {MAX_URL_BYTES} bytes"
            ));
        }
        validate_http_url("jina__read_url.url", url)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fathom_env::Action;
    use serde_json::json;

    use super::JinaReadUrlAction;

    #[test]
    fn validate_rejects_empty_url() {
        let action = JinaReadUrlAction;
        let error = action
            .validate(&json!({ "url": "   " }))
            .expect_err("empty url must fail");
        assert!(error.contains("url"));
    }

    #[test]
    fn validate_rejects_non_http_scheme() {
        let action = JinaReadUrlAction;
        let error = action
            .validate(&json!({ "url": "file:///tmp/a.txt" }))
            .expect_err("file scheme must fail");
        assert!(error.contains("http or https"));
    }

    #[test]
    fn validate_rejects_relative_url() {
        let action = JinaReadUrlAction;
        let error = action
            .validate(&json!({ "url": "/docs/page" }))
            .expect_err("relative url must fail");
        assert!(error.contains("valid URL"));
    }

    #[test]
    fn validate_accepts_https_url() {
        let action = JinaReadUrlAction;
        assert!(
            action
                .validate(&json!({ "url": "https://example.com/path" }))
                .is_ok()
        );
    }
}
