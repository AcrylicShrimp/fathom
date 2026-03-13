use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

pub(crate) const SHELL_RUN_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(0);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: SHELL_RUN_ACTION_KEY,
        action_name: "run",
        description: "Run one non-interactive shell command in a relative working directory under the current base path. Supports optional environment overrides; non-zero exit code marks the execution as failed.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "path": { "type": "string" },
                "env": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            },
            "required": ["command"],
            "additionalProperties": false
        }),
    }
}
