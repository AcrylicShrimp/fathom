use fathom_capability_domain::CapabilityActionDefinition;
use serde_json::json;

use super::common::system_spec;

pub(super) fn definition() -> CapabilityActionDefinition {
    system_spec(
        2,
        "read_execution_input",
        "Read a byte-range slice from the serialized input payload of one execution.",
        json!({
            "type": "object",
            "properties": {
                "execution_id": { "type": "string" },
                "offset": { "type": "integer", "minimum": 0 },
                "limit": { "type": "integer", "minimum": 0 }
            },
            "required": ["execution_id", "limit"],
            "additionalProperties": false
        }),
    )
}
