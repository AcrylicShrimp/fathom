use fathom_capability_domain::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{
    args_object, optional_boolean, optional_non_empty_string, optional_non_empty_string_list,
    optional_u64, require_non_empty_string, require_relative_path,
};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_CAPABILITY_DOMAIN_ID,
};

const SEARCH_MAX_RESULTS_CAP: u64 = 10_000;

pub struct FsSearchAction;

impl Action for FsSearchAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            capability_domain_id: FILESYSTEM_CAPABILITY_DOMAIN_ID,
            action_name: "search",
            description: "Find regex matches inside UTF-8 files under the current base path. Optionally scope the search path, include patterns, case sensitivity, and result count.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "include": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "max_results": { "type": "integer", "minimum": 1 },
                    "case_sensitive": { "type": "boolean" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            discovery: false,
            mode_support: ActionModeSupport::AwaitOnly,
            max_timeout_ms: FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
            desired_timeout_ms: Some(FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS),
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_non_empty_string(args, "pattern")?;
        if optional_non_empty_string(args, "path")?.is_some() {
            require_relative_path(args, "path")?;
        }
        optional_non_empty_string_list(args, "include")?;
        optional_boolean(args, "case_sensitive")?;
        if let Some(max_results) = optional_u64(args, "max_results")? {
            if max_results == 0 {
                return Err("filesystem__search.max_results must be >= 1".to_string());
            }
            if max_results > SEARCH_MAX_RESULTS_CAP {
                return Err(format!(
                    "filesystem__search.max_results must be <= {SEARCH_MAX_RESULTS_CAP}"
                ));
            }
        }
        Ok(())
    }
}
