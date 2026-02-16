use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, require_non_empty_string, system_spec};

pub(super) struct GetProfileAction;

impl Action for GetProfileAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_profile",
            "Get a single agent/user profile by id.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["agent", "user"] },
                    "id": { "type": "string" },
                    "view": { "type": "string", "enum": ["summary", "full"] }
                },
                "required": ["kind", "id", "view"],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;

        let kind = require_non_empty_string(args, "kind")?;
        if kind != "agent" && kind != "user" {
            return Err("system__get_profile.kind must be `agent` or `user`".to_string());
        }

        require_non_empty_string(args, "id")?;

        let view = require_non_empty_string(args, "view")?;
        if view != "summary" && view != "full" {
            return Err("system__get_profile.view must be `summary` or `full`".to_string());
        }

        Ok(())
    }
}
