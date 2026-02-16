use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{
    args_object, optional_u64, require_non_empty_string, require_relative_path, require_string,
};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_ENVIRONMENT_ID,
};

pub struct FsReplaceAction;

impl Action for FsReplaceAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "replace",
            description: "Replace UTF-8 text in a base-path-relative file path. Requires `path`, non-empty `old`, string `new`, `mode` in {`first`,`all`}; optional `expected_replacements` guard.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old": { "type": "string" },
                    "new": { "type": "string" },
                    "mode": { "type": "string", "enum": ["first", "all"] },
                    "expected_replacements": { "type": "integer", "minimum": 0 }
                },
                "required": ["path", "old", "new", "mode"],
                "additionalProperties": false
            }),
            discovery: false,
            max_timeout_ms: FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
            desired_timeout_ms: Some(FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS),
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_relative_path(args, "path")?;
        require_non_empty_string(args, "old")?;
        require_string(args, "new")?;
        let mode = require_non_empty_string(args, "mode")?;
        if mode != "first" && mode != "all" {
            return Err("filesystem__replace.mode must be `first` or `all`".to_string());
        }
        optional_u64(args, "expected_replacements")?;
        Ok(())
    }
}
