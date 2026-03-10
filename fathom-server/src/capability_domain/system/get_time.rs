use fathom_capability_domain::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, system_spec};

pub(super) struct GetTimeAction;

impl Action for GetTimeAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_time",
            "Get the latest server clock time context (UTC and local timezone).",
            json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        if !args.is_empty() {
            return Err("system__get_time does not accept arguments".to_string());
        }
        Ok(())
    }
}
