use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::FILESYSTEM_ENVIRONMENT_ID;
use crate::validate::args_object;

pub struct FsGetBasePathAction;

impl Action for FsGetBasePathAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "get_base_path",
            description: "Return the current filesystem environment base path.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
            discovery: true,
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        if !args.is_empty() {
            return Err("filesystem__get_base_path does not accept arguments".to_string());
        }
        Ok(())
    }
}
