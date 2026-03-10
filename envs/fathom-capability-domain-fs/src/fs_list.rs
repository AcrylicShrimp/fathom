use fathom_capability_domain::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::validate::{args_object, optional_boolean, optional_u64, require_relative_path};
use crate::{
    FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS, FILESYSTEM_ACTION_MAX_TIMEOUT_MS,
    FILESYSTEM_CAPABILITY_DOMAIN_ID,
};

const LIST_MAX_ENTRIES_CAP: u64 = 5_000;

pub struct FsListAction;

impl Action for FsListAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            capability_domain_id: FILESYSTEM_CAPABILITY_DOMAIN_ID,
            action_name: "list",
            description: "List files/directories at a base-path-relative location. `path` must be a non-empty relative path; use `.` for the root directory. Optional controls: `recursive`, `max_entries`, `include_hidden`.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "recursive": { "type": "boolean" },
                    "max_entries": { "type": "integer", "minimum": 1 },
                    "include_hidden": { "type": "boolean" }
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
        optional_boolean(args, "recursive")?;
        optional_boolean(args, "include_hidden")?;
        if let Some(max_entries) = optional_u64(args, "max_entries")? {
            if max_entries == 0 {
                return Err("filesystem__list.max_entries must be >= 1".to_string());
            }
            if max_entries > LIST_MAX_ENTRIES_CAP {
                return Err(format!(
                    "filesystem__list.max_entries must be <= {LIST_MAX_ENTRIES_CAP}"
                ));
            }
        }
        Ok(())
    }
}
