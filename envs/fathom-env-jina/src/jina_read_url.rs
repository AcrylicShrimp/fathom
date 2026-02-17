use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{
    args_object, optional_boolean, optional_non_empty_string, optional_u64,
    require_non_empty_string, validate_http_url,
};
use crate::{
    JINA_ACTION_DESIRED_TIMEOUT_MS, JINA_ACTION_MAX_TIMEOUT_MS, JINA_ENVIRONMENT_ID,
    JINA_TOKEN_BUDGET_MAX,
};

const MAX_URL_BYTES: usize = 8_192;
const MAX_SELECTOR_BYTES: usize = 4_096;
const MAX_COOKIE_BYTES: usize = 16_384;

pub struct JinaReadUrlAction;

impl Action for JinaReadUrlAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: JINA_ENVIRONMENT_ID,
            action_name: "read_url",
            description: "Read one absolute http(s) URL via Jina Reader API. The environment applies two-stage defaults: hard selector profile first, then soft no-selector fallback on provider/transport failures. Advanced filter fields are optional and must be omitted when unused (do not send empty strings).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "target_selector": { "type": "string", "minLength": 1 },
                    "remove_selector": { "type": "string", "minLength": 1 },
                    "wait_for_selector": { "type": "string", "minLength": 1 },
                    "set_cookie": { "type": "string", "minLength": 1 },
                    "no_cache": { "type": "boolean" },
                    "token_budget": { "type": "integer", "minimum": 1, "maximum": JINA_TOKEN_BUDGET_MAX },
                    "timeout_ms": { "type": "integer", "minimum": 1, "maximum": JINA_ACTION_MAX_TIMEOUT_MS }
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

        if let Some(selector) = optional_non_empty_string(args, "target_selector")?
            && selector.len() > MAX_SELECTOR_BYTES
        {
            return Err(format!(
                "jina__read_url.target_selector must be <= {MAX_SELECTOR_BYTES} bytes"
            ));
        }
        if let Some(selector) = optional_non_empty_string(args, "remove_selector")?
            && selector.len() > MAX_SELECTOR_BYTES
        {
            return Err(format!(
                "jina__read_url.remove_selector must be <= {MAX_SELECTOR_BYTES} bytes"
            ));
        }
        if let Some(selector) = optional_non_empty_string(args, "wait_for_selector")?
            && selector.len() > MAX_SELECTOR_BYTES
        {
            return Err(format!(
                "jina__read_url.wait_for_selector must be <= {MAX_SELECTOR_BYTES} bytes"
            ));
        }
        if let Some(cookie) = optional_non_empty_string(args, "set_cookie")?
            && cookie.len() > MAX_COOKIE_BYTES
        {
            return Err(format!(
                "jina__read_url.set_cookie must be <= {MAX_COOKIE_BYTES} bytes"
            ));
        }
        let _ = optional_boolean(args, "no_cache")?;
        if let Some(token_budget) = optional_u64(args, "token_budget")?
            && !(1..=JINA_TOKEN_BUDGET_MAX).contains(&token_budget)
        {
            return Err(format!(
                "jina__read_url.token_budget must be in range [1, {JINA_TOKEN_BUDGET_MAX}]"
            ));
        }
        if let Some(timeout_ms) = optional_u64(args, "timeout_ms")?
            && !(1..=JINA_ACTION_MAX_TIMEOUT_MS).contains(&timeout_ms)
        {
            return Err(format!(
                "jina__read_url.timeout_ms must be in range [1, {JINA_ACTION_MAX_TIMEOUT_MS}]"
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fathom_env::Action;
    use serde_json::{Value, json};

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

    #[test]
    fn validate_accepts_advanced_options() {
        let action = JinaReadUrlAction;
        assert!(
            action
                .validate(&json!({
                    "url": "https://example.com/path",
                    "target_selector": "main, section, article",
                    "remove_selector": ".cookie, .banner",
                    "wait_for_selector": "main",
                    "set_cookie": "foo=bar",
                    "no_cache": true,
                    "token_budget": 200000,
                    "timeout_ms": 5000
                }))
                .is_ok()
        );
    }

    #[test]
    fn validate_rejects_invalid_advanced_options() {
        let action = JinaReadUrlAction;
        let error = action
            .validate(&json!({
                "url": "https://example.com/path",
                "target_selector": ""
            }))
            .expect_err("empty target_selector must fail");
        assert!(error.contains("target_selector"));
        assert!(error.contains("must be omitted or a non-empty string"));

        let error = action
            .validate(&json!({
                "url": "https://example.com/path",
                "token_budget": 0
            }))
            .expect_err("token_budget=0 must fail");
        assert!(error.contains("token_budget"));
    }

    #[test]
    fn schema_optional_string_fields_require_min_length() {
        let action = JinaReadUrlAction;
        let spec = action.spec();
        let properties = spec
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("schema properties");

        for key in [
            "target_selector",
            "remove_selector",
            "wait_for_selector",
            "set_cookie",
        ] {
            assert_eq!(properties[key]["type"], json!("string"));
            assert_eq!(properties[key]["minLength"], json!(1));
        }

        let required = spec
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .expect("schema required");
        assert!(required.iter().any(|value| value.as_str() == Some("url")));
        assert!(
            !required
                .iter()
                .any(|value| value.as_str() == Some("target_selector"))
        );
        assert!(
            !required
                .iter()
                .any(|value| value.as_str() == Some("remove_selector"))
        );
        assert!(
            !required
                .iter()
                .any(|value| value.as_str() == Some("wait_for_selector"))
        );
        assert!(
            !required
                .iter()
                .any(|value| value.as_str() == Some("set_cookie"))
        );
    }
}
