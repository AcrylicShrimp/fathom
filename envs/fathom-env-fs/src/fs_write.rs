use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::FILESYSTEM_ENVIRONMENT_ID;
use crate::validate::{args_object, require_boolean, require_relative_path, require_string};

pub struct FsWriteAction;

impl Action for FsWriteAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "write",
            description: "Write text content to a base-path-relative file path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "allow_override": { "type": "boolean" }
                },
                "required": ["path", "content", "allow_override"],
                "additionalProperties": false
            }),
            discovery: false,
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_relative_path(args, "path")?;
        require_string(args, "content")?;
        require_boolean(args, "allow_override")?;
        Ok(())
    }
}
