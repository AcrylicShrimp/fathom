use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

use crate::{JINA_ACTION_MAX_TIMEOUT_MS, JINA_TOKEN_BUDGET_MAX};

pub(crate) const JINA_READ_URL_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(0);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: JINA_READ_URL_ACTION_KEY,
        action_name: "read_url",
        description: "Read one absolute HTTP(S) URL and return extracted page content as markdown plus source metadata. Optional selector and budget fields can tighten extraction when a page is noisy or large.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "target_selector": { "type": "string", "minLength": 1 },
                "remove_selector": { "type": "string", "minLength": 1 },
                "wait_for_selector": { "type": "string", "minLength": 1 },
                "token_budget": { "type": "integer", "minimum": 1, "maximum": JINA_TOKEN_BUDGET_MAX },
                "timeout_ms": { "type": "integer", "minimum": 1, "maximum": JINA_ACTION_MAX_TIMEOUT_MS }
            },
            "required": ["url"],
            "additionalProperties": false
        }),
    }
}
