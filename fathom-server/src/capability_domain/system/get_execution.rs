use fathom_capability_domain::CapabilityActionDefinition;
use serde_json::json;

use super::common::system_spec;

pub(super) fn definition() -> CapabilityActionDefinition {
    system_spec(
        1,
        "get_execution",
        "Inspect one execution in detail, including its current state, input preview, and result preview when available.",
        json!({
            "type": "object",
            "properties": {
                "execution_id": { "type": "string" }
            },
            "required": ["execution_id"],
            "additionalProperties": false
        }),
    )
}
