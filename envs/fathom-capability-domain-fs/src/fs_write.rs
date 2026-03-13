use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_WRITE_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(3);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_WRITE_ACTION_KEY,
        action_name: "write",
        description: "Create or overwrite a UTF-8 text file at a relative path under the current base path. `allow_override` controls whether an existing file may be replaced.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "allow_override": { "type": "boolean" },
                "create_parents": { "type": "boolean" }
            },
            "required": ["path", "content", "allow_override"],
            "additionalProperties": false
        }),
    }
}
