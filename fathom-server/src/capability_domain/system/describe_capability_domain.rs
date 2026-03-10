use fathom_capability_domain::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, require_non_empty_string, system_spec};

pub(super) struct DescribeCapabilityDomainAction;

impl Action for DescribeCapabilityDomainAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "describe_capability_domain",
            "Describe one activated capability domain, including its intent, capabilities, actions, and recipes.",
            json!({
                "type": "object",
                "properties": {
                    "capability_domain_id": { "type": "string" }
                },
                "required": ["capability_domain_id"],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_non_empty_string(args, "capability_domain_id")?;
        Ok(())
    }
}
