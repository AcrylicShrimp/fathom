use fathom_capability_domain::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{
    args_object, optional_boolean, require_boolean, require_relative_path, require_string,
};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_CAPABILITY_DOMAIN_ID,
};

pub struct FsWriteAction;

impl Action for FsWriteAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            capability_domain_id: FILESYSTEM_CAPABILITY_DOMAIN_ID,
            action_name: "write",
            description: "Create or overwrite a UTF-8 text file at a relative path under the current base path. `allow_override` controls whether an existing file may be replaced.",
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
            mode_support: ActionModeSupport::AwaitOnly,
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
