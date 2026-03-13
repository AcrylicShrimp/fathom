mod constants;
mod execute;
mod shell_run;

use std::path::PathBuf;
use std::time::Instant;

use fathom_capability_domain::{
    CapabilityActionDefinition, CapabilityActionResult, CapabilityActionSubmission,
    CapabilityDomainRecipe, CapabilityDomainSessionContext, CapabilityDomainSpec, DomainFactory,
    DomainInstance, DomainInstanceFuture,
};
use serde_json::{Value, json};

use crate::constants::ACTION_DESIRED_TIMEOUT_MS;
pub const SHELL_CAPABILITY_DOMAIN_ID: &str = "shell";
pub use execute::execute_action;

pub struct ShellDomainFactory {
    base_path: PathBuf,
    execution_timeout_ms: u64,
}

impl ShellDomainFactory {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            execution_timeout_ms: ACTION_DESIRED_TIMEOUT_MS,
        }
    }
}

impl DomainFactory for ShellDomainFactory {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: SHELL_CAPABILITY_DOMAIN_ID,
            name: "Shell",
            description: "Workspace-scoped shell capability domain rooted at a base path. Runs non-interactive commands in base-path-relative directories with bounded output and runtime-managed timeouts.",
            schema_version: 1,
        }
    }

    fn actions(&self) -> Vec<CapabilityActionDefinition> {
        vec![shell_run::definition()]
    }

    fn create_instance(
        &self,
        _session_context: CapabilityDomainSessionContext,
    ) -> Box<dyn DomainInstance> {
        Box::new(ShellDomainInstance::new(
            self.base_path.clone(),
            self.execution_timeout_ms,
        ))
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Run a bounded diagnostic command".to_string(),
                steps: vec![
                    "Call `shell__run` with one focused non-interactive command and `path: \".\"` when the domain root is the intended working directory.".to_string(),
                    "Inspect `exit_code`, `stdout`, and `stderr` in the result before deciding the next step.".to_string(),
                    "If output is truncated, rerun with a narrower command so the missing detail fits in one result.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Run work in a specific directory".to_string(),
                steps: vec![
                    "Set `path` to the non-empty relative directory where the command should run.".to_string(),
                    "Keep the command scoped to one task so failures are easy to interpret.".to_string(),
                    "If the command fails, adjust the command or working directory and rerun with a narrower goal.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Run with environment overrides".to_string(),
                steps: vec![
                    "Provide `env` only for variables the command actually depends on.".to_string(),
                    "Use valid environment keys and string values only.".to_string(),
                    "If the command times out, narrow the command, reduce output, or break the work into smaller commands.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Start longer-running shell work".to_string(),
                steps: vec![
                    "Use `shell__run` when the command may continue beyond the current turn.".to_string(),
                    "Request detached execution only when the result is not required before responding.".to_string(),
                    "Keep the command and working directory focused so later status and result updates remain interpretable.".to_string(),
                ],
            },
        ]
    }
}

struct ShellDomainInstance {
    state: Value,
    execution_timeout_ms: u64,
}

impl ShellDomainInstance {
    fn new(base_path: PathBuf, execution_timeout_ms: u64) -> Self {
        Self {
            state: json!({
                "base_path": base_path.to_string_lossy().to_string(),
            }),
            execution_timeout_ms,
        }
    }
}

impl DomainInstance for ShellDomainInstance {
    fn execute_actions<'a>(
        &'a mut self,
        submissions: Vec<CapabilityActionSubmission>,
    ) -> DomainInstanceFuture<'a> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(submissions.len());
            for submission in submissions {
                results.push(
                    execute_submission(&self.state, self.execution_timeout_ms, submission).await,
                );
            }
            results
        })
    }
}

async fn execute_submission(
    state: &Value,
    execution_timeout_ms: u64,
    submission: CapabilityActionSubmission,
) -> CapabilityActionResult {
    let Some(action_name) = action_name_for_key(submission.action_key) else {
        return CapabilityActionResult::runtime_error(
            "unknown_action_key",
            format!(
                "shell domain instance does not recognize action key {}",
                submission.action_key.0
            ),
            None,
            0,
        );
    };
    let args_json = match serde_json::to_string(&submission.args) {
        Ok(args_json) => args_json,
        Err(error) => {
            return CapabilityActionResult::runtime_error(
                "invalid_submission_args",
                format!("failed to serialize action arguments: {error}"),
                None,
                0,
            );
        }
    };

    let started_at = Instant::now();
    let mut result = execute_action(action_name, &args_json, state, execution_timeout_ms)
        .await
        .unwrap_or_else(|| {
            CapabilityActionResult::runtime_error(
                "unknown_action",
                format!("shell action `{action_name}` is not implemented"),
                None,
                0,
            )
        });
    if result.execution_time_ms == 0 {
        result.execution_time_ms =
            started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    }
    result
}

fn action_name_for_key(key: fathom_capability_domain::CapabilityActionKey) -> Option<&'static str> {
    match key {
        shell_run::SHELL_RUN_ACTION_KEY => Some("run"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ShellDomainFactory, shell_run};
    use fathom_capability_domain::{
        CapabilityActionSubmission, CapabilityDomainSessionContext, DomainFactory,
    };
    use serde_json::json;

    #[tokio::test]
    async fn shell_factory_instance_executes_run_action() {
        let mut instance = ShellDomainFactory::new(
            std::env::current_dir().expect("current directory for shell factory"),
        )
        .create_instance(CapabilityDomainSessionContext {
            session_id: "session-test".to_string(),
        });

        let results = instance
            .execute_actions(vec![CapabilityActionSubmission {
                action_key: shell_run::SHELL_RUN_ACTION_KEY,
                args: json!({ "command": "pwd", "path": "." }),
            }])
            .await;

        assert_eq!(results.len(), 1);
        assert!(results[0].outcome.is_ok());
    }
}
