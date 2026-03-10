use fathom_capability_domain::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, require_non_empty_string, system_spec};

pub(super) struct ListProfilesAction;

impl Action for ListProfilesAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "list_profiles",
            "List agent and/or user profiles in the runtime.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["agent", "user", "all"] }
                },
                "required": ["kind"],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        let kind = require_non_empty_string(args, "kind")?;
        if kind != "agent" && kind != "user" && kind != "all" {
            return Err("system__list_profiles.kind must be `agent`, `user`, or `all`".to_string());
        }
        Ok(())
    }
}
