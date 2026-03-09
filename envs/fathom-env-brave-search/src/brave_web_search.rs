use fathom_env::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{args_object, optional_u64, require_non_empty_string};
use crate::{
    BRAVE_SEARCH_ACTION_DESIRED_TIMEOUT_MS, BRAVE_SEARCH_ACTION_MAX_TIMEOUT_MS,
    BRAVE_SEARCH_ENVIRONMENT_ID, BRAVE_SEARCH_MAX_COUNT,
};

const MAX_QUERY_BYTES: usize = 4_096;

pub struct BraveWebSearchAction;

impl Action for BraveWebSearchAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: BRAVE_SEARCH_ENVIRONMENT_ID,
            action_name: "web_search",
            description: "Search the web via Brave Search API with compact ranked results. Input requires non-empty `query`; optional `count` controls number of results.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "count": { "type": "integer", "minimum": 1, "maximum": BRAVE_SEARCH_MAX_COUNT }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            discovery: false,
            mode_support: ActionModeSupport::AwaitOnly,
            max_timeout_ms: BRAVE_SEARCH_ACTION_MAX_TIMEOUT_MS,
            desired_timeout_ms: Some(BRAVE_SEARCH_ACTION_DESIRED_TIMEOUT_MS),
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        let query = require_non_empty_string(args, "query")?;
        if query.len() > MAX_QUERY_BYTES {
            return Err(format!(
                "brave_search__web_search.query must be <= {MAX_QUERY_BYTES} bytes"
            ));
        }

        if let Some(count) = optional_u64(args, "count")?
            && (count == 0 || count > u64::from(BRAVE_SEARCH_MAX_COUNT))
        {
            return Err(format!(
                "brave_search__web_search.count must be in range [1, {BRAVE_SEARCH_MAX_COUNT}]"
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::BraveWebSearchAction;
    use fathom_env::Action;

    #[test]
    fn validate_rejects_empty_query() {
        let action = BraveWebSearchAction;
        let error = action
            .validate(&json!({ "query": "  " }))
            .expect_err("empty query must fail");
        assert!(error.contains("query"));
    }

    #[test]
    fn validate_rejects_count_out_of_range() {
        let action = BraveWebSearchAction;
        let error = action
            .validate(&json!({ "query": "rust", "count": 0 }))
            .expect_err("count=0 must fail");
        assert!(error.contains("count"));

        let error = action
            .validate(&json!({ "query": "rust", "count": 9999 }))
            .expect_err("count too large must fail");
        assert!(error.contains("count"));
    }

    #[test]
    fn validate_accepts_minimal_and_bounded_args() {
        let action = BraveWebSearchAction;
        assert!(
            action
                .validate(&json!({ "query": "rust tonic grpc" }))
                .is_ok()
        );
        assert!(
            action
                .validate(&json!({ "query": "rust", "count": 20 }))
                .is_ok()
        );
    }
}
