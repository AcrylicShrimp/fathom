use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_LIST_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(1);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_LIST_ACTION_KEY,
        action_name: "list",
        description: "List directory entries at a non-empty relative path under the current base path; use `.` for the root directory. Supports recursive listing, hidden entries, and bounded results.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "recursive": { "type": "boolean" },
                "max_entries": { "type": "integer", "minimum": 1 },
                "include_hidden": { "type": "boolean" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
    }
}
