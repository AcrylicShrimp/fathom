use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const FS_GET_BASE_PATH_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(0);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: FS_GET_BASE_PATH_ACTION_KEY,
        action_name: "get_base_path",
        description: "Return the current base path for this filesystem domain.",
        input_schema: json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        }),
    }
}
