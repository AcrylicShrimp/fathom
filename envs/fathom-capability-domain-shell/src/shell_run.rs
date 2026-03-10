use fathom_capability_domain::{Action, ActionModeSupport, ActionSpec};
use serde_json::{Value, json};

use crate::SHELL_CAPABILITY_DOMAIN_ID;
use crate::constants::{
    ACTION_DESIRED_TIMEOUT_MS, ACTION_MAX_TIMEOUT_MS, MAX_COMMAND_BYTES, MAX_ENV_VARS,
    is_valid_env_key,
};
use crate::validate::{
    args_object, optional_non_empty_string, optional_object, require_non_empty_string,
    validate_relative_path,
};

pub struct ShellRunAction;

impl Action for ShellRunAction {
    fn spec(&self) -> ActionSpec {
        ActionSpec {
            capability_domain_id: SHELL_CAPABILITY_DOMAIN_ID,
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
            discovery: false,
            mode_support: ActionModeSupport::AwaitOrDetach,
            max_timeout_ms: ACTION_MAX_TIMEOUT_MS,
            desired_timeout_ms: Some(ACTION_DESIRED_TIMEOUT_MS),
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
