use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, system_spec};

pub(super) struct GetContextAction;

impl Action for GetContextAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_context",
            "Get authoritative runtime/session context and activated environment summaries.",
            json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let _ = args_object(args)?;
        Ok(())
    }
}
