use fathom_env::{Action, ActionSpec};
use serde_json::{Value, json};

use crate::SHELL_ENVIRONMENT_ID;
use crate::constants::{MAX_COMMAND_BYTES, MAX_ENV_VARS, MAX_TIMEOUT_MS, is_valid_env_key};
use crate::validate::{
    args_object, optional_non_empty_string, optional_object, optional_u64,
    require_non_empty_string, validate_relative_path,
};

pub struct ShellRunAction;

impl Action for ShellRunAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            environment_id: SHELL_ENVIRONMENT_ID,
            action_name: "run",
            description: "Execute one non-interactive shell command at a base-path-relative working directory. Non-zero exit code marks the task as failed.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "path": { "type": "string" },
                    "env": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    },
                    "timeout_ms": { "type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_MS }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            discovery: false,
        }
    }

    fn validate(&self, args: &Value) -> Result<(), String> {
        let args = args_object(args)?;
        let command = require_non_empty_string(args, "command")?;
        if command.len() > MAX_COMMAND_BYTES {
            return Err(format!(
                "shell__run.command must be <= {MAX_COMMAND_BYTES} bytes"
            ));
        }

        if let Some(path) = optional_non_empty_string(args, "path")? {
            validate_relative_path("path", path)?;
        }

        if let Some(timeout_ms) = optional_u64(args, "timeout_ms")? {
            if timeout_ms == 0 {
                return Err("shell__run.timeout_ms must be >= 1".to_string());
            }
            if timeout_ms > MAX_TIMEOUT_MS {
                return Err(format!("shell__run.timeout_ms must be <= {MAX_TIMEOUT_MS}"));
            }
        }

        if let Some(env) = optional_object(args, "env")? {
            if env.len() > MAX_ENV_VARS {
                return Err(format!(
                    "shell__run.env supports up to {MAX_ENV_VARS} entries"
                ));
            }
            for (key, value) in env {
                if !is_valid_env_key(key) {
                    return Err(format!(
                        "shell__run.env key `{key}` is invalid (must match [A-Za-z_][A-Za-z0-9_]*)"
                    ));
                }
                if value.as_str().is_none() {
                    return Err(format!(
                        "shell__run.env value for key `{key}` must be a string"
                    ));
                }
            }
        }

        Ok(())
    }
}
