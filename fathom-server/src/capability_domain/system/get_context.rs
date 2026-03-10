use fathom_capability_domain::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, system_spec};

pub(super) struct GetContextAction;

impl Action for GetContextAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_context",
            "Return authoritative runtime and session context, including current server time and activated capability domain summaries.",
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
