use fathom_capability_domain::CapabilityActionDefinition;
use serde_json::json;

use super::common::system_spec;

pub(super) fn definition() -> CapabilityActionDefinition {
    system_spec(
        0,
        "list_executions",
        "List execution summaries for the current session with cursor pagination and optional exact filters.",
        json!({
            "type": "object",
            "properties": {
                "cursor": { "type": "string" },
                "limit": { "type": "integer", "minimum": 0 },
                "state": {
                    "type": "string",
                    "enum": [
                        "queued",
                        "running_foreground",
                        "running_background",
                        "succeeded",
                        "failed",
                        "canceled"
                    ]
                },
                "action_id": { "type": "string" }
            },
            "additionalProperties": false
        }),
    )
}
