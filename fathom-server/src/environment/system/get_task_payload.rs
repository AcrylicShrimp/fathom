use fathom_tooling::{Action, ActionCall, ActionFuture, ActionSpec};
use serde_json::{Value, json};

use super::common::{
    args_object, execute_system, require_non_empty_string, require_optional_u64, system_spec,
};

pub(super) struct GetTaskPayloadAction;

impl Action for GetTaskPayloadAction {
    fn spec(&self) -> ActionSpec {
        system_spec(
            "get_task_payload",
            "Lookup the full args/result payload for a task using task_id and part.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "part": { "type": "string", "enum": ["args", "result"] },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 0 }
                },
                "required": ["task_id", "part"],
                "additionalProperties": false
            }),
        )
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;

        require_non_empty_string(args, "task_id")?;

        let part = require_non_empty_string(args, "part")?;
        if part != "args" && part != "result" {
            return Err("system__get_task_payload.part must be `args` or `result`".to_string());
        }

        require_optional_u64(
            args,
            "offset",
            "system__get_task_payload.offset must be a non-negative integer",
        )?;
        require_optional_u64(
            args,
            "limit",
            "system__get_task_payload.limit must be a non-negative integer",
        )?;

        Ok(())
    }

    fn execute<'a>(&'a self, call: ActionCall<'a>) -> ActionFuture<'a> {
        execute_system(call, "get_task_payload")
    }
}
