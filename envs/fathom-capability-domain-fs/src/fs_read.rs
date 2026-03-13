use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_READ_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(2);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_READ_ACTION_KEY,
        action_name: "read",
        description: "Read UTF-8 text from a relative file path under the current base path. Supports line-windowed reads for large files.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "offset_line": { "type": "integer", "minimum": 1 },
                "limit_lines": { "type": "integer", "minimum": 1 }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
    }
}
