use fathom_capability_domain::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, system_spec};

pub(super) struct GetSessionIdentityMapAction;

impl Action for GetSessionIdentityMapAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_session_identity_map",
            "Return the active session identity references for the current agent and participants.",
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
