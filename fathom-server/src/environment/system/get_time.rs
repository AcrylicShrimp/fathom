use fathom_env::{Action, ActionCall, ActionFuture, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, execute_system, system_spec};

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

    fn execute<'a>(&'a self, call: ActionCall<'a>) -> ActionFuture<'a> {
        execute_system(call, "get_time")
    }
}
