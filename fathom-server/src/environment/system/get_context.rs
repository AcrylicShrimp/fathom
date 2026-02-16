use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, system_spec};

pub(super) struct GetContextAction;

impl Action for GetContextAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_context",
            "Get authoritative runtime/session context, activated environments, and policy hints.",
            json!({
                "type": "object",
                "properties": {
                    "include_actions": { "type": "boolean" }
                },
                "required": [],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        if let Some(include_actions) = args.get("include_actions")
            && !include_actions.is_boolean()
        {
            return Err("system__get_context.include_actions must be a boolean".to_string());
        }
        Ok(())
    }
}
