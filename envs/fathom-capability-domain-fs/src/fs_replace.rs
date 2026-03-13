use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_REPLACE_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(4);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_REPLACE_ACTION_KEY,
        action_name: "replace",
        description: "Apply literal string replacement to a UTF-8 text file at a relative path under the current base path. Supports `first` and `all` modes plus an optional `expected_replacements` guard.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old": { "type": "string" },
                "new": { "type": "string" },
                "mode": { "type": "string", "enum": ["first", "all"] },
                "expected_replacements": { "type": "integer", "minimum": 0 }
            },
            "required": ["path", "old", "new", "mode"],
            "additionalProperties": false
        }),
    }
}
