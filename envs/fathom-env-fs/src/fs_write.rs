use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{
    args_object, optional_boolean, require_boolean, require_relative_path, require_string,
};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_ENVIRONMENT_ID,
};

pub struct FsWriteAction;

impl Action for FsWriteAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "write",
            description: "Write text content to a base-path-relative file path. Requires `path`, `content`, `allow_override`; optional `create_parents` (default true).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "allow_override": { "type": "boolean" },
                    "create_parents": { "type": "boolean" }
                },
                "required": ["path", "content", "allow_override"],
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
        require_string(args, "content")?;
        require_boolean(args, "allow_override")?;
        optional_boolean(args, "create_parents")?;
        Ok(())
    }
}
