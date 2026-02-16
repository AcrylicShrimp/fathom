use fathom_tooling::{Action, ActionCall, ActionFuture, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, execute_system, system_spec};

pub(super) struct GetSessionIdentityMapAction;

impl Action for GetSessionIdentityMapAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_session_identity_map",
            "Get active session identity references (agent and participants).",
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

    fn execute<'a>(&'a self, call: ActionCall<'a>) -> ActionFuture<'a> {
        execute_system(call, "get_session_identity_map")
    }
}
