use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::environment::{EnvironmentRegistry, ResolvedAction};
use crate::runtime::Runtime;
use crate::session::state::SessionCommand;
use crate::session::task_context::TaskExecutionContext;
use crate::util::now_unix_ms;

use fathom_env::{ActionOutcome, EnvironmentSnapshot, FinalizedAction};

const EXECUTION_TIMEOUT_GRACE_MS: u64 = 250;

#[derive(Debug, Clone)]
pub(crate) struct EnvironmentCommittedAction {
    pub(crate) task_id: String,
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) env_seq: u64,
    pub(crate) succeeded: bool,
    pub(crate) message: String,
    pub(crate) state_patch_applied: bool,
    pub(crate) state_snapshot: EnvironmentSnapshot,
}

#[derive(Clone)]
pub(crate) struct EnvironmentActorHandle {
    pub(crate) environment_id: String,
    command_tx: mpsc::Sender<EnvironmentActorCommand>,
}

impl EnvironmentActorHandle {
    pub(crate) async fn submit(&self, submission: EnvironmentActionSubmission) {
        let _ = self
            .command_tx
            .send(EnvironmentActorCommand::Submit(submission))
            .await;
    }
}

#[derive(Clone)]
pub(crate) struct EnvironmentActionSubmission {
    pub(crate) task_id: String,
    pub(crate) env_seq: u64,
    pub(crate) resolved_action: ResolvedAction,
    pub(crate) args_json: String,
    pub(crate) execution_context: TaskExecutionContext,
}

pub(crate) enum EnvironmentActorCommand {
    Submit(EnvironmentActionSubmission),
    ExecutionFinished(EnvironmentFinishedExecution),
}

#[derive(Clone)]
pub(crate) struct EnvironmentFinishedExecution {
    task_id: String,
    env_seq: u64,
    resolved_action: ResolvedAction,
    args_json: String,
    outcome: ActionOutcome,
}

pub(crate) fn spawn_environment_actor(
    runtime: Runtime,
    environment_id: String,
    initial_snapshot: EnvironmentSnapshot,
    session_command_tx: mpsc::Sender<SessionCommand>,
) -> EnvironmentActorHandle {
    let (command_tx, command_rx) = mpsc::channel(128);
    let handle = EnvironmentActorHandle {
        environment_id,
        command_tx: command_tx.clone(),
    };

    tokio::spawn(run_environment_actor(
        runtime,
        command_tx,
        command_rx,
        session_command_tx,
        initial_snapshot,
    ));

    handle
}

async fn run_environment_actor(
    runtime: Runtime,
    command_tx: mpsc::Sender<EnvironmentActorCommand>,
    mut command_rx: mpsc::Receiver<EnvironmentActorCommand>,
    session_command_tx: mpsc::Sender<SessionCommand>,
    mut snapshot: EnvironmentSnapshot,
) {
    let mut pending = VecDeque::<EnvironmentActionSubmission>::new();
    let mut completed = BTreeMap::<u64, EnvironmentFinishedExecution>::new();
    let mut running_count = 0usize;
    let mut next_commit_seq = 1u64;

    while let Some(command) = command_rx.recv().await {
        match command {
            EnvironmentActorCommand::Submit(submission) => {
                pending.push_back(submission);
                maybe_start_pending(
                    &runtime,
                    &command_tx,
                    &snapshot,
                    &mut pending,
                    &mut running_count,
                );
            }
            EnvironmentActorCommand::ExecutionFinished(execution) => {
                running_count = running_count.saturating_sub(1);
                completed.insert(execution.env_seq, execution);

                while let Some(done) = completed.remove(&next_commit_seq) {
                    let finalized = FinalizedAction {
                        seq: done.env_seq,
                        canonical_action_id: done.resolved_action.canonical_action_id.clone(),
                        action_name: done.resolved_action.action_name.clone(),
                        args_json: done.args_json.clone(),
                        succeeded: done.outcome.succeeded,
                        message: done.outcome.message.clone(),
                        state_patch: done.outcome.state_patch.clone(),
                    };

                    let (succeeded, message, state_patch_applied) =
                        match EnvironmentRegistry::apply_transition(
                            &done.resolved_action,
                            &snapshot.state_json,
                            &finalized,
                        ) {
                            Ok(transition) => {
                                if let Some(patch) = transition.state_patch {
                                    apply_json_merge_patch(&mut snapshot.state_json, &patch);
                                    (done.outcome.succeeded, done.outcome.message, true)
                                } else {
                                    (done.outcome.succeeded, done.outcome.message, false)
                                }
                            }
                            Err(error) => (
                                false,
                                format!("environment transition failed: {error}"),
                                false,
                            ),
                        };

                    snapshot.updated_at_unix_ms = now_unix_ms();

                    let committed = EnvironmentCommittedAction {
                        task_id: done.task_id,
                        canonical_action_id: done.resolved_action.canonical_action_id,
                        environment_id: done.resolved_action.environment_id,
                        action_name: done.resolved_action.action_name,
                        env_seq: done.env_seq,
                        succeeded,
                        message,
                        state_patch_applied,
                        state_snapshot: snapshot.clone(),
                    };

                    let _ = session_command_tx
                        .send(SessionCommand::EnvironmentActionCommitted { committed })
                        .await;

                    next_commit_seq += 1;
                }

                maybe_start_pending(
                    &runtime,
                    &command_tx,
                    &snapshot,
                    &mut pending,
                    &mut running_count,
                );
            }
        }
    }
}

fn maybe_start_pending(
    runtime: &Runtime,
    command_tx: &mpsc::Sender<EnvironmentActorCommand>,
    snapshot: &EnvironmentSnapshot,
    pending: &mut VecDeque<EnvironmentActionSubmission>,
    running_count: &mut usize,
) {
    let max_parallel = runtime.task_capacity();
    while *running_count < max_parallel {
        let Some(submission) = pending.pop_front() else {
            break;
        };
        *running_count += 1;

        let effective_timeout_ms = match submission
            .resolved_action
            .timeout_policy
            .effective_timeout_ms()
        {
            Ok(timeout_ms) => timeout_ms,
            Err(error) => {
                let finished = EnvironmentFinishedExecution {
                    task_id: submission.task_id,
                    env_seq: submission.env_seq,
                    resolved_action: submission.resolved_action,
                    args_json: submission.args_json,
                    outcome: timeout_policy_failure_outcome(&error),
                };
                let command_tx = command_tx.clone();
                tokio::spawn(async move {
                    let _ = command_tx
                        .send(EnvironmentActorCommand::ExecutionFinished(finished))
                        .await;
                });
                continue;
            }
        };

        let runtime = runtime.clone();
        let command_tx = command_tx.clone();
        let environment_state = snapshot.state_json.clone();

        tokio::spawn(async move {
            let timeout_with_grace_ms =
                effective_timeout_ms.saturating_add(EXECUTION_TIMEOUT_GRACE_MS);
            let timeout_duration = Duration::from_millis(timeout_with_grace_ms);
            let action_future = EnvironmentRegistry::execute_action(
                &runtime,
                &submission.execution_context,
                &submission.resolved_action,
                &submission.args_json,
                &environment_state,
                effective_timeout_ms,
            );
            let outcome = match tokio::time::timeout(timeout_duration, action_future).await {
                Ok(Some(outcome)) => outcome,
                Ok(None) => ActionOutcome {
                    succeeded: false,
                    message: format!(
                        "environment action `{}` execution unavailable",
                        submission.resolved_action.canonical_action_id
                    ),
                    state_patch: None,
                },
                Err(_) => timeout_exceeded_outcome(
                    &submission.resolved_action.canonical_action_id,
                    effective_timeout_ms,
                ),
            };

            let finished = EnvironmentFinishedExecution {
                task_id: submission.task_id,
                env_seq: submission.env_seq,
                resolved_action: submission.resolved_action,
                args_json: submission.args_json,
                outcome,
            };

            let _ = command_tx
                .send(EnvironmentActorCommand::ExecutionFinished(finished))
                .await;
        });
    }
}

fn timeout_policy_failure_outcome(reason: &str) -> ActionOutcome {
    ActionOutcome {
        succeeded: false,
        message: json!({
            "ok": false,
            "error_code": "timeout_policy_invalid",
            "message": reason,
        })
        .to_string(),
        state_patch: None,
    }
}

fn timeout_exceeded_outcome(canonical_action_id: &str, timeout_ms: u64) -> ActionOutcome {
    ActionOutcome {
        succeeded: false,
        message: json!({
            "ok": false,
            "error_code": "timeout_exceeded",
            "canonical_action_id": canonical_action_id,
            "timeout_ms": timeout_ms,
            "message": format!("action execution exceeded timeout of {timeout_ms}ms"),
        })
        .to_string(),
        state_patch: None,
    }
}

fn apply_json_merge_patch(target: &mut Value, patch: &Value) {
    if let Value::Object(patch_object) = patch {
        if !target.is_object() {
            *target = Value::Object(serde_json::Map::new());
        }
        let target_object = target
            .as_object_mut()
            .expect("target must be object after initialization");

        for (key, value) in patch_object {
            if value.is_null() {
                target_object.remove(key);
            } else if let Some(target_value) = target_object.get_mut(key) {
                apply_json_merge_patch(target_value, value);
            } else {
                target_object.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = patch.clone();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::apply_json_merge_patch;

    #[test]
    fn merge_patch_overwrites_leaf_and_removes_null_fields() {
        let mut target = json!({"a":1,"b":{"x":2,"y":3},"c":9});
        let patch = json!({"a":2,"b":{"x":null,"z":4},"c":null});

        apply_json_merge_patch(&mut target, &patch);

        assert_eq!(target, json!({"a":2,"b":{"y":3,"z":4}}));
    }
}
