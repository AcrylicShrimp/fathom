use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::json;

use crate::BRAVE_SEARCH_MAX_COUNT;

pub(crate) const BRAVE_WEB_SEARCH_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(0);

pub(crate) fn definition() -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: BRAVE_WEB_SEARCH_ACTION_KEY,
        action_name: "web_search",
        description: "Run a web search query and return compact ranked result metadata. Use `count` to bound how many results are returned.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "count": { "type": "integer", "minimum": 1, "maximum": BRAVE_SEARCH_MAX_COUNT }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
    }
}
