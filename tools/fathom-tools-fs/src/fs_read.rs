use fathom_tooling::{Action, ActionCall, ActionFuture, ActionSpec};
use serde_json::{Value, json};

use crate::FILESYSTEM_ENVIRONMENT_ID;
use crate::validate::{args_object, require_managed_or_fs_path};

pub struct FsReadAction;

impl Action for FsReadAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "read",
            description: "Read text content from a managed:// or fs:// file path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            discovery: false,
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_managed_or_fs_path(args, "path")?;
        Ok(())
    }

    fn execute<'a>(&'a self, call: ActionCall<'a>) -> ActionFuture<'a> {
        call.host
            .execute_environment_action(FILESYSTEM_ENVIRONMENT_ID, "read", call.args_json)
    }
}
