use fathom_env::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{args_object, optional_u64, require_relative_path};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_ENVIRONMENT_ID,
};

const READ_MAX_LIMIT_LINES: u64 = 2_000;

pub struct FsReadAction;

impl Action for FsReadAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: FILESYSTEM_ENVIRONMENT_ID,
            action_name: "read",
            description: "Read UTF-8 text content from a base-path-relative file path with optional line windowing (`offset_line`, `limit_lines`).",
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
            discovery: false,
            mode_support: ActionModeSupport::AwaitOnly,
            max_timeout_ms: FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
            desired_timeout_ms: Some(FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS),
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        require_relative_path(args, "path")?;
        if let Some(offset_line) = optional_u64(args, "offset_line")?
            && offset_line == 0
        {
            return Err("filesystem__read.offset_line must be >= 1".to_string());
        }
        if let Some(limit_lines) = optional_u64(args, "limit_lines")? {
            if limit_lines == 0 {
                return Err("filesystem__read.limit_lines must be >= 1".to_string());
            }
            if limit_lines > READ_MAX_LIMIT_LINES {
                return Err(format!(
                    "filesystem__read.limit_lines must be <= {READ_MAX_LIMIT_LINES}"
                ));
            }
        }
        Ok(())
    }
}
