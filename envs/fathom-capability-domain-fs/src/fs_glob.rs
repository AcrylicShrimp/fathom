use fathom_capability_domain::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{
    args_object, optional_boolean, optional_non_empty_string, optional_u64,
    require_non_empty_string, require_relative_path,
};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_CAPABILITY_DOMAIN_ID,
};

const GLOB_MAX_RESULTS_CAP: u64 = 5_000;

pub struct FsGlobAction;

impl Action for FsGlobAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            capability_domain_id: FILESYSTEM_CAPABILITY_DOMAIN_ID,
            action_name: "glob",
            description: "Find files matching a glob pattern under a relative path. Requires non-empty `pattern`; optional `path`, `max_results`, `include_hidden`.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1 },
                    "include_hidden": { "type": "boolean" }
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
        optional_boolean(args, "include_hidden")?;
        if let Some(max_results) = optional_u64(args, "max_results")? {
            if max_results == 0 {
                return Err("filesystem__glob.max_results must be >= 1".to_string());
            }
            if max_results > GLOB_MAX_RESULTS_CAP {
                return Err(format!(
                    "filesystem__glob.max_results must be <= {GLOB_MAX_RESULTS_CAP}"
                ));
            }
        }
        Ok(())
    }
}
