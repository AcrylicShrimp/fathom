mod error;
mod path;
mod real;
mod result;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use fathom_capability_domain::ActionOutcome;
use serde::Deserialize;
use serde_json::{Value, json};

use self::error::ShellError;
use self::path::{ParsedPath, parse_path, resolve_target_dir};
use self::real::{RealRunOptions, run_command};
use crate::constants::{
    DEFAULT_MAX_STDERR_BYTES, DEFAULT_MAX_STDOUT_BYTES, MAX_COMMAND_BYTES, MAX_ENV_VARS,
    is_valid_env_key,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunArgs {
    command: String,
    path: Option<String>,
    env: Option<BTreeMap<String, String>>,
}

pub async fn execute_action(
    action_name: &str,
    args_json: &str,
    capability_domain_state: &Value,
    execution_timeout_ms: u64,
) -> Option<ActionOutcome> {
    match action_name {
        "run" => Some(execute_run(args_json, capability_domain_state, execution_timeout_ms).await),
        _ => None,
    }
}

async fn execute_run(
    args_json: &str,
    capability_domain_state: &Value,
    execution_timeout_ms: u64,
) -> ActionOutcome {
    let args = match parse_args::<RunArgs>(args_json, "shell__run") {
        Ok(args) => args,
        Err(error) => return result::failure("run", None, &error, None),
    };

    let command = args.command.trim();
    if command.is_empty() {
        let error = ShellError::invalid_args("shell__run.command must be a non-empty string");
        return result::failure("run", None, &error, None);
    }
    if command.len() > MAX_COMMAND_BYTES {
        let error = ShellError::invalid_args(format!(
            "shell__run.command must be <= {MAX_COMMAND_BYTES} bytes"
        ));
        return result::failure("run", None, &error, None);
    }

    let path = args.path.unwrap_or_else(|| ".".to_string());
    let parsed = match parse_path(&path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("run", Some(path.as_str()), &error, None),
    };

    let timeout_ms = execution_timeout_ms;
    if timeout_ms == 0 {
        let error = ShellError::invalid_args("shell execution timeout must be greater than zero");
        return result::failure("run", Some(parsed.normalized_path()), &error, None);
    }
    let env_overrides = match parse_env_overrides(args.env) {
        Ok(value) => value,
        Err(error) => {
            return result::failure("run", Some(parsed.normalized_path()), &error, None);
        }
    };

    execute_run_on_path(
        parsed,
        command,
        timeout_ms,
        env_overrides,
        capability_domain_state,
    )
    .await
}

async fn execute_run_on_path(
    parsed: ParsedPath,
    command: &str,
    timeout_ms: u64,
    env_overrides: BTreeMap<String, String>,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let (_base, target_dir) = match resolve_target_dir(capability_domain_state, &parsed.rel_path) {
        Ok(paths) => paths,
        Err(error) => {
            return result::failure("run", Some(parsed.normalized_path()), &error, None);
        }
    };

    let options = RealRunOptions {
        timeout_ms,
        env_overrides,
        max_stdout_bytes: DEFAULT_MAX_STDOUT_BYTES,
        max_stderr_bytes: DEFAULT_MAX_STDERR_BYTES,
    };

    let execution = match run_command(command, &target_dir, &options).await {
        Ok(execution) => execution,
        Err(error) => {
            return result::failure("run", Some(parsed.normalized_path()), &error, None);
        }
    };

    let data = json!({
        "command": command,
        "effective_cwd": target_dir.display().to_string(),
        "exit_code": execution.exit_code,
        "stdout": execution.stdout.text,
        "stderr": execution.stderr.text,
        "stdout_truncated_bytes": execution.stdout.truncated_bytes,
        "stderr_truncated_bytes": execution.stderr.truncated_bytes,
        "duration_ms": execution.duration_ms,
        "timed_out": execution.timed_out,
    });

    if execution.timed_out {
        let error = ShellError::timeout(format!("command timed out after {timeout_ms}ms"))
            .with_details(json!({ "timeout_ms": timeout_ms }));
        return result::failure("run", Some(parsed.normalized_path()), &error, Some(data));
    }

    if execution.exit_code != Some(0) {
        let error = ShellError::execution_failed(format!(
            "command exited with non-zero status {}",
            format_exit_code(execution.exit_code)
        ));
        return result::failure("run", Some(parsed.normalized_path()), &error, Some(data));
    }

    result::success("run", parsed.normalized_path(), data)
}

fn parse_env_overrides(
    env: Option<BTreeMap<String, String>>,
) -> Result<BTreeMap<String, String>, ShellError> {
    let env = env.unwrap_or_default();
    if env.len() > MAX_ENV_VARS {
        return Err(ShellError::invalid_args(format!(
            "shell__run.env supports up to {MAX_ENV_VARS} entries"
        )));
    }

    for key in env.keys() {
        if !is_valid_env_key(key) {
            return Err(ShellError::invalid_args(format!(
                "shell__run.env key `{key}` is invalid (must match [A-Za-z_][A-Za-z0-9_]*)"
            )));
        }
    }

    Ok(env)
}

fn parse_args<T: for<'de> Deserialize<'de>>(
    args_json: &str,
    action_id: &str,
) -> Result<T, ShellError> {
    serde_json::from_str(args_json).map_err(|error| {
        ShellError::invalid_args(format!("{action_id} arguments are invalid: {error}"))
    })
}

fn format_exit_code(code: Option<i32>) -> String {
    code.map(|value| value.to_string())
        .unwrap_or_else(|| "terminated by signal".to_string())
}
