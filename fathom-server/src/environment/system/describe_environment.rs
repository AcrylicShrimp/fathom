use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, require_non_empty_string, system_spec};

pub(super) struct DescribeEnvironmentAction;

impl Action for DescribeEnvironmentAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "describe_environment",
            "Describe one activated environment, including intent, capabilities, actions, and recipes.",
            json!({
                "type": "object",
                "properties": {
                    "env_id": { "type": "string" }
                },
                "required": ["env_id"],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_non_empty_string(args, "env_id")?;
        Ok(())
    }
}
