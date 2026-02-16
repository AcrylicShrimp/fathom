use fathom_env::{Action, ActionCall, ActionFuture, ActionSpec};
use serde_json::{Value, json};

use crate::FILESYSTEM_ENVIRONMENT_ID;
use crate::validate::{args_object, require_boolean, require_managed_or_fs_path, require_string};

pub struct FsWriteAction;

impl Action for FsWriteAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "write",
            description: "Write text content to a managed:// or fs:// file path.",
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
        require_managed_or_fs_path(args, "path")?;
        require_string(args, "content")?;
        require_boolean(args, "allow_override")?;
        Ok(())
    }

    fn execute<'a>(&'a self, call: ActionCall<'a>) -> ActionFuture<'a> {
        call.host
            .execute_environment_action(FILESYSTEM_ENVIRONMENT_ID, "write", call.args_json)
    }
}
