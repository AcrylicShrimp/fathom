use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_SEARCH_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(6);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_SEARCH_ACTION_KEY,
        action_name: "search",
        description: "Find regex matches inside UTF-8 files under the current base path. Optionally scope the search path, include patterns, case sensitivity, and result count.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "include": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "max_results": { "type": "integer", "minimum": 1 },
                "case_sensitive": { "type": "boolean" }
            },
            "required": ["pattern"],
            "additionalProperties": false
        }),
    }
}
