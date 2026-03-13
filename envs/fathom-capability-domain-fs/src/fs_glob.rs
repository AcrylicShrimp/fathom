use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_GLOB_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(5);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_GLOB_ACTION_KEY,
        action_name: "glob",
        description: "Find paths under the current base path that match a glob pattern. Optionally scope the search path, include hidden entries, and bound the result count.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "max_results": { "type": "integer", "minimum": 1 },
                "include_hidden": { "type": "boolean" }
            },
            "required": ["pattern"],
            "additionalProperties": false
        }),
    }
}
