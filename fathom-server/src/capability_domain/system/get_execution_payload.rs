use fathom_capability_domain::{Action, ActionSpec};
use serde_json::{Value, json};

use super::common::{args_object, require_non_empty_string, require_optional_u64, system_spec};

pub(super) struct GetExecutionPayloadAction;

impl Action for GetExecutionPayloadAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_execution_payload",
            "Return full execution args or result payload data for a specific execution id, with optional paging.",
            json!({
                "type": "object",
                "properties": {
                    "execution_id": { "type": "string" },
                    "part": { "type": "string", "enum": ["args", "result"] },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 0 }
                },
                "required": ["execution_id", "part"],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;

        require_non_empty_string(args, "execution_id")?;

        let part = require_non_empty_string(args, "part")?;
        if part != "args" && part != "result" {
            return Err(
                "system__get_execution_payload.part must be `args` or `result`".to_string(),
            );
        }

        require_optional_u64(
            args,
            "offset",
            "system__get_execution_payload.offset must be a non-negative integer",
        )?;
        require_optional_u64(
            args,
            "limit",
            "system__get_execution_payload.limit must be a non-negative integer",
        )?;

        Ok(())
    }
}
