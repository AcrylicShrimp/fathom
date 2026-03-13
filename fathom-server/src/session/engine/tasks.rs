use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::Instant;
use tonic::Status;

use crate::agent::ActionInvocation;
use crate::capability_domain::{
    CapabilityDomainActionExecution, CapabilityDomainActionSubmission, CapabilityDomainActorHandle,
    CapabilityDomainCommittedAction, ResolvedAction,
};
use crate::history;
use crate::runtime::Runtime;
use crate::session::diagnostics::execution_to_json;
use crate::session::payload_lookup::resolve_from_execution;
use crate::session::state::{
    ExecutionRuntimeState, ExecutionSubmissionExecution, ExecutionSubmissionState,
    ExecutionSubmissionStatus, SessionState,
};
use crate::util::now_unix_ms;
use fathom_capability_domain::{ActionError, CapabilityActionResult};
use fathom_protocol::pb;
use fathom_protocol::{execution_status_label, execution_update_phase_label};
use serde_json::json;

use super::events::{emit_event, emit_execution_update_event, enqueue_trigger};

pub(super) struct QueuedExecution {
    pub(super) execution: pb::Execution,
    pub(super) outcome: QueuedExecutionOutcome,
    pub(super) call_key: String,
    pub(super) call_id: Option<String>,
}

pub(super) enum QueuedExecutionOutcome {
    ForegroundAccepted,
    BackgroundAccepted,
    Rejected,
}

pub(super) enum CommitTurnPolicy {
    ResumeNow,
    DeferUntilFutureTrigger,
}

const FOREGROUND_WAIT_BUDGET: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct AcceptedExecution {
    execution_id: String,
    resolved_action: ResolvedAction,
    background_requested: bool,
    call_key: String,
    call_id: Option<String>,
}

struct AcceptedExecutionGroup {
    capability_domain_id: String,
    executions: Vec<AcceptedExecution>,
    all_background_requested: bool,
}

pub(super) fn queue_executions(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
    action_invocations: Vec<ActionInvocation>,
) -> Vec<QueuedExecution> {
    let mut queued_executions = Vec::with_capacity(action_invocations.len());
    let mut grouped = Vec::<AcceptedExecutionGroup>::new();
    let mut grouped_positions = HashMap::<String, usize>::new();

    for action_invocation in action_invocations {
        let ActionInvocation {
            action_id,
            args_json,
            call_key,
            call_id,
        } = action_invocation;
        let execution_id = runtime.next_execution_id();
        let now = now_unix_ms();
        let mut execution = pb::Execution {
            execution_id: execution_id.clone(),
            session_id: state.session_id.clone(),
            action_id: action_id.clone(),
            args_json: args_json.clone(),
            status: pb::ExecutionStatus::Pending as i32,
            result_message: String::new(),
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        let mut outcome = QueuedExecutionOutcome::Rejected;

        match background_requested_from_args_json(&args_json) {
            Ok(background_requested) => {
                let resolved = runtime.capability_domain_registry().resolve(&action_id);
                if let Some(resolved_action) = resolved {
                    if !state
                        .engaged_capability_domain_ids
                        .contains(&resolved_action.capability_domain_id)
                    {
                        execution.status = pb::ExecutionStatus::Failed as i32;
                        execution.result_message = format!(
                            "environment `{}` is not engaged for this session",
                            resolved_action.capability_domain_id
                        );
                    } else if capability_domain_handles
                        .contains_key(&resolved_action.capability_domain_id)
                    {
                        outcome = if background_requested {
                            QueuedExecutionOutcome::BackgroundAccepted
                        } else {
                            QueuedExecutionOutcome::ForegroundAccepted
                        };
                        let group_index = if let Some(index) =
                            grouped_positions.get(&resolved_action.capability_domain_id)
                        {
                            *index
                        } else {
                            let index = grouped.len();
                            grouped_positions
                                .insert(resolved_action.capability_domain_id.clone(), index);
                            grouped.push(AcceptedExecutionGroup {
                                capability_domain_id: resolved_action.capability_domain_id.clone(),
                                executions: Vec::new(),
                                all_background_requested: true,
                            });
                            index
                        };
                        let group = grouped
                            .get_mut(group_index)
                            .expect("group index must be valid");
                        group.all_background_requested &= background_requested;
                        group.executions.push(AcceptedExecution {
                            execution_id: execution_id.clone(),
                            resolved_action,
                            background_requested,
                            call_key: call_key.clone(),
                            call_id: call_id.clone(),
                        });
                    } else {
                        execution.status = pb::ExecutionStatus::Failed as i32;
                        execution.result_message = format!(
                            "environment runtime `{}` is unavailable",
                            resolved_action.capability_domain_id
                        );
                    }
                } else {
                    execution.status = pb::ExecutionStatus::Failed as i32;
                    execution.result_message = format!("unknown action `{action_id}`");
                }
            }
            Err(error) => {
                execution.status = pb::ExecutionStatus::Failed as i32;
                execution.result_message = error;
            }
        }

        state
            .executions
            .insert(execution_id.clone(), execution.clone());
        emit_execution_state_changed(state, events_tx, &execution);
        history::append_execution_requested_history(state, &execution);
        append_execution_started_record(runtime, state, &execution);

        if matches!(outcome, QueuedExecutionOutcome::Rejected) {
            append_execution_rejected_record(runtime, state, &execution);
            enqueue_execution_update_trigger(
                runtime,
                state,
                events_tx,
                build_execution_update_trigger(
                    runtime,
                    &execution.execution_id,
                    &execution.action_id,
                    pb::ExecutionUpdateKind::ExecutionRejected,
                    execution.result_message.clone(),
                    String::new(),
                ),
            );
        } else if matches!(outcome, QueuedExecutionOutcome::BackgroundAccepted) {
            enqueue_execution_update_trigger(
                runtime,
                state,
                events_tx,
                build_execution_update_trigger(
                    runtime,
                    &execution.execution_id,
                    &execution.action_id,
                    pb::ExecutionUpdateKind::ExecutionBackgrounded,
                    String::new(),
                    String::new(),
                ),
            );
        }

        queued_executions.push(QueuedExecution {
            execution,
            outcome,
            call_key,
            call_id,
        });
    }

    for group in grouped {
        let submission_id = runtime.next_execution_submission_id();
        let submission_background = group.all_background_requested;
        let running_now = !state
            .active_submission_ids_by_domain
            .contains_key(&group.capability_domain_id);
        let submission_status = match (running_now, submission_background) {
            (true, true) => ExecutionSubmissionStatus::RunningBackground,
            (true, false) => ExecutionSubmissionStatus::RunningForeground,
            (false, _) => ExecutionSubmissionStatus::Queued,
        };

        state.execution_submissions.insert(
            submission_id.clone(),
            ExecutionSubmissionState {
                capability_domain_id: group.capability_domain_id.clone(),
                executions: group
                    .executions
                    .iter()
                    .map(|execution| ExecutionSubmissionExecution {
                        execution_id: execution.execution_id.clone(),
                        action_key: execution.resolved_action.action_key,
                    })
                    .collect(),
                status: submission_status,
                foreground_wait_deadline: (!submission_background)
                    .then(|| Instant::now() + FOREGROUND_WAIT_BUDGET),
            },
        );
        if !submission_background {
            state
                .foreground_submission_ids
                .insert(submission_id.clone());
        }
        for accepted in &group.executions {
            state.execution_runtimes.insert(
                accepted.execution_id.clone(),
                ExecutionRuntimeState {
                    submission_id: submission_id.clone(),
                    background_requested: accepted.background_requested,
                    call_key: accepted.call_key.clone(),
                    call_id: accepted.call_id.clone(),
                },
            );
        }

        if running_now {
            state
                .active_submission_ids_by_domain
                .insert(group.capability_domain_id.clone(), submission_id.clone());
            start_execution_submission(
                state,
                events_tx,
                capability_domain_handles,
                &group.capability_domain_id,
                &submission_id,
            );
        } else {
            state
                .queued_submission_ids_by_domain
                .entry(group.capability_domain_id.clone())
                .or_default()
                .push_back(submission_id.clone());
        }
    }

    queued_executions
}

pub(super) fn cancel_execution(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
    execution_id: &str,
) -> Result<pb::CancelExecutionResponse, Status> {
    let Some(execution) = state.executions.get(execution_id) else {
        return Err(Status::not_found("execution not found"));
    };

    let status =
        pb::ExecutionStatus::try_from(execution.status).unwrap_or(pb::ExecutionStatus::Unspecified);
    let is_terminal = matches!(
        status,
        pb::ExecutionStatus::Succeeded
            | pb::ExecutionStatus::Failed
            | pb::ExecutionStatus::Canceled
    );
    if is_terminal {
        return Ok(pb::CancelExecutionResponse {
            canceled: false,
            execution: Some(execution.clone()),
        });
    }

    let submission_id = state
        .execution_runtimes
        .get(execution_id)
        .map(|runtime| runtime.submission_id.clone());
    let Some(submission_id) = submission_id else {
        return Ok(pb::CancelExecutionResponse {
            canceled: false,
            execution: Some(execution.clone()),
        });
    };
    let Some(submission) = state.execution_submissions.remove(&submission_id) else {
        return Ok(pb::CancelExecutionResponse {
            canceled: false,
            execution: Some(execution.clone()),
        });
    };

    state.foreground_submission_ids.remove(&submission_id);
    if state
        .active_submission_ids_by_domain
        .get(&submission.capability_domain_id)
        .is_some_and(|active_submission_id| active_submission_id == &submission_id)
    {
        state
            .active_submission_ids_by_domain
            .remove(&submission.capability_domain_id);
        start_next_queued_submission(
            runtime,
            state,
            events_tx,
            capability_domain_handles,
            &submission.capability_domain_id,
        );
    } else if let Some(queue) = state
        .queued_submission_ids_by_domain
        .get_mut(&submission.capability_domain_id)
    {
        queue.retain(|queued_submission_id| queued_submission_id != &submission_id);
        if queue.is_empty() {
            state
                .queued_submission_ids_by_domain
                .remove(&submission.capability_domain_id);
        }
    }

    let mut canceled_execution = None;
    for submission_execution in submission.executions {
        let submission_execution_id = submission_execution.execution_id;
        state.execution_runtimes.remove(&submission_execution_id);
        if let Some(execution) = state.executions.get_mut(&submission_execution_id) {
            execution.status = pb::ExecutionStatus::Canceled as i32;
            execution.result_message = "canceled by request".to_string();
            execution.updated_at_unix_ms = now_unix_ms();
            let execution_snapshot = execution.clone();
            emit_execution_state_changed(state, events_tx, &execution_snapshot);
            if submission_execution_id == execution_id {
                canceled_execution = Some(execution_snapshot);
            }
        }
    }

    Ok(pb::CancelExecutionResponse {
        canceled: canceled_execution.is_some(),
        execution: canceled_execution,
    })
}

pub(super) fn handle_capability_domain_action_committed(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
    committed: CapabilityDomainCommittedAction,
) -> CommitTurnPolicy {
    if !state
        .engaged_capability_domain_ids
        .contains(&committed.capability_domain_id)
    {
        return CommitTurnPolicy::DeferUntilFutureTrigger;
    }

    let Some(submission) = state.execution_submissions.remove(&committed.submission_id) else {
        return CommitTurnPolicy::DeferUntilFutureTrigger;
    };
    let submission_is_foreground = !matches!(
        submission.status,
        ExecutionSubmissionStatus::RunningBackground
    );

    state
        .foreground_submission_ids
        .remove(&committed.submission_id);
    if state
        .active_submission_ids_by_domain
        .get(&submission.capability_domain_id)
        .is_some_and(|active_submission_id| active_submission_id == &committed.submission_id)
    {
        state
            .active_submission_ids_by_domain
            .remove(&submission.capability_domain_id);
    }

    for committed_execution in committed.executions {
        settle_committed_execution(runtime, state, events_tx, committed_execution);
    }

    start_next_queued_submission(
        runtime,
        state,
        events_tx,
        capability_domain_handles,
        &submission.capability_domain_id,
    );

    if submission_is_foreground {
        CommitTurnPolicy::ResumeNow
    } else {
        CommitTurnPolicy::DeferUntilFutureTrigger
    }
}

pub(super) fn background_expired_submissions(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
) -> bool {
    let now = Instant::now();
    let expired_submission_ids = state
        .foreground_submission_ids
        .iter()
        .filter(|submission_id| {
            state
                .execution_submissions
                .get(*submission_id)
                .and_then(|submission| submission.foreground_wait_deadline)
                .is_some_and(|deadline| deadline <= now)
        })
        .cloned()
        .collect::<Vec<_>>();

    if expired_submission_ids.is_empty() {
        return false;
    }

    for submission_id in expired_submission_ids {
        background_submission(runtime, state, events_tx, &submission_id);
    }
    true
}

fn emit_execution_state_changed(
    state: &SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    execution: &pb::Execution,
) {
    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::ExecutionStateChanged(pb::ExecutionStateChangedEvent {
            execution: Some(execution.clone()),
        }),
    );
}

fn append_execution_started_record(
    runtime: &Runtime,
    state: &SessionState,
    execution: &pb::Execution,
) {
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "execution.started",
            "session_id": state.session_id,
            "execution": execution_to_json(execution),
        }),
    );
}

fn append_execution_rejected_record(
    runtime: &Runtime,
    state: &SessionState,
    execution: &pb::Execution,
) {
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "execution.rejected",
            "session_id": state.session_id,
            "execution": execution_to_json(execution),
        }),
    );
}

fn start_execution_submission(
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
    capability_domain_id: &str,
    submission_id: &str,
) {
    let submission_background = !state.foreground_submission_ids.contains(submission_id);
    let Some(submission) = state.execution_submissions.get_mut(submission_id) else {
        return;
    };
    submission.status = if submission_background {
        ExecutionSubmissionStatus::RunningBackground
    } else {
        ExecutionSubmissionStatus::RunningForeground
    };
    if submission_background {
        submission.foreground_wait_deadline = None;
    }
    let submission_executions = submission.executions.clone();

    let mut execution_snapshots = Vec::new();
    let now = now_unix_ms();
    for submission_execution in &submission_executions {
        let Some(execution) = state.executions.get_mut(&submission_execution.execution_id) else {
            continue;
        };
        if execution.status != pb::ExecutionStatus::Running as i32 {
            execution.status = pb::ExecutionStatus::Running as i32;
            execution.updated_at_unix_ms = now;
            execution_snapshots.push(execution.clone());
        }
    }
    for execution in &execution_snapshots {
        emit_execution_state_changed(state, events_tx, execution);
    }

    let Some(handle) = capability_domain_handles.get(capability_domain_id) else {
        return;
    };
    let submission = CapabilityDomainActionSubmission {
        submission_id: submission_id.to_string(),
        executions: submission_executions
            .into_iter()
            .filter_map(|submission_execution| {
                state
                    .executions
                    .get(&submission_execution.execution_id)
                    .map(|execution| CapabilityDomainActionExecution {
                        execution_id: submission_execution.execution_id,
                        action_key: submission_execution.action_key,
                        args_json: execution.args_json.clone(),
                    })
            })
            .collect(),
    };
    let handle = handle.clone();
    tokio::spawn(async move {
        handle.submit(submission).await;
    });
}

fn background_submission(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    submission_id: &str,
) {
    let Some(submission) = state.execution_submissions.get_mut(submission_id) else {
        return;
    };
    if !state.foreground_submission_ids.remove(submission_id) {
        return;
    }
    submission.foreground_wait_deadline = None;
    if matches!(
        submission.status,
        ExecutionSubmissionStatus::RunningForeground
    ) {
        submission.status = ExecutionSubmissionStatus::RunningBackground;
    }
    let submission_executions = submission.executions.clone();

    for submission_execution in submission_executions {
        let Some(execution_runtime) = state
            .execution_runtimes
            .get_mut(&submission_execution.execution_id)
        else {
            continue;
        };
        execution_runtime.background_requested = true;
        let Some(execution) = state.executions.get(&submission_execution.execution_id) else {
            continue;
        };
        let detail =
            settled_execution_output(execution, pb::ExecutionUpdatePhase::ExecutionBackgrounded);
        emit_execution_update_event(
            events_tx,
            &state.session_id,
            pb::ExecutionUpdatePhase::ExecutionBackgrounded,
            execution_runtime.call_key.clone(),
            execution_runtime.call_id.clone(),
            Some(execution.action_id.clone()),
            Some(execution.execution_id.clone()),
            String::new(),
            String::new(),
            detail,
        );
        enqueue_execution_update_trigger(
            runtime,
            state,
            events_tx,
            build_execution_update_trigger(
                runtime,
                &execution.execution_id,
                &execution.action_id,
                pb::ExecutionUpdateKind::ExecutionBackgrounded,
                String::new(),
                String::new(),
            ),
        );
    }
}

fn start_next_queued_submission(
    _runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
    capability_domain_id: &str,
) {
    let (next_submission_id, queue_is_empty) = if let Some(queue) = state
        .queued_submission_ids_by_domain
        .get_mut(capability_domain_id)
    {
        (queue.pop_front(), queue.is_empty())
    } else {
        (None, false)
    };
    if queue_is_empty {
        state
            .queued_submission_ids_by_domain
            .remove(capability_domain_id);
    }
    let Some(next_submission_id) = next_submission_id else {
        return;
    };
    state
        .active_submission_ids_by_domain
        .insert(capability_domain_id.to_string(), next_submission_id.clone());
    start_execution_submission(
        state,
        events_tx,
        capability_domain_handles,
        capability_domain_id,
        &next_submission_id,
    );
}

fn settle_committed_execution(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    committed_execution: crate::capability_domain::CapabilityDomainCommittedExecution,
) {
    let Some(execution_runtime) = state
        .execution_runtimes
        .remove(&committed_execution.execution_id)
    else {
        return;
    };
    let Some(execution) = state.executions.get_mut(&committed_execution.execution_id) else {
        return;
    };
    let status =
        pb::ExecutionStatus::try_from(execution.status).unwrap_or(pb::ExecutionStatus::Unspecified);
    if status == pb::ExecutionStatus::Canceled {
        return;
    }
    if !matches!(
        status,
        pb::ExecutionStatus::Running | pb::ExecutionStatus::Pending
    ) {
        return;
    }

    let succeeded = action_result_succeeded(&committed_execution.result);
    execution.status = if succeeded {
        pb::ExecutionStatus::Succeeded as i32
    } else {
        pb::ExecutionStatus::Failed as i32
    };
    execution.result_message = serialize_action_result_message(&committed_execution.result);
    execution.updated_at_unix_ms = now_unix_ms();
    let execution_snapshot = execution.clone();

    emit_execution_state_changed(state, events_tx, &execution_snapshot);
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "execution.finished",
            "session_id": state.session_id,
            "execution": execution_to_json(&execution_snapshot),
        }),
    );
    if let Some(lookup) = resolve_from_execution(&execution_snapshot) {
        state.push_pending_payload_lookup(lookup);
    }

    let (trigger_kind, phase, _) =
        outcome_phase_for_commit(execution_runtime.background_requested, succeeded);
    let detail = settled_execution_output(&execution_snapshot, phase);

    emit_execution_update_event(
        events_tx,
        &state.session_id,
        phase,
        execution_runtime.call_key,
        execution_runtime.call_id,
        Some(execution_snapshot.action_id.clone()),
        Some(execution_snapshot.execution_id.clone()),
        String::new(),
        String::new(),
        detail,
    );
    enqueue_execution_update_trigger(
        runtime,
        state,
        events_tx,
        build_execution_update_trigger(
            runtime,
            &execution_snapshot.execution_id,
            &execution_snapshot.action_id,
            trigger_kind,
            if succeeded {
                String::new()
            } else {
                execution_snapshot.result_message.clone()
            },
            execution_snapshot.result_message.clone(),
        ),
    );
}

fn enqueue_execution_update_trigger(
    _runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    trigger: pb::Trigger,
) {
    enqueue_trigger(state, events_tx, trigger);
}

fn build_execution_update_trigger(
    runtime: &Runtime,
    execution_id: &str,
    action_id: &str,
    kind: pb::ExecutionUpdateKind,
    message: String,
    payload_message: String,
) -> pb::Trigger {
    pb::Trigger {
        trigger_id: runtime.next_trigger_id(),
        created_at_unix_ms: now_unix_ms(),
        kind: Some(pb::trigger::Kind::ExecutionUpdate(
            pb::ExecutionUpdateTrigger {
                execution_id: execution_id.to_string(),
                action_id: action_id.to_string(),
                kind: kind as i32,
                message,
                payload_message,
            },
        )),
    }
}

pub(super) fn settled_execution_output(
    execution: &pb::Execution,
    phase: pb::ExecutionUpdatePhase,
) -> String {
    let status = pb::ExecutionStatus::try_from(execution.status)
        .map(execution_status_label)
        .unwrap_or("unknown");
    match phase {
        pb::ExecutionUpdatePhase::ExecutionSucceeded => format!(
            "{} execution `{}` finished as {}",
            execution_update_phase_label(phase),
            execution.execution_id,
            status
        ),
        pb::ExecutionUpdatePhase::ExecutionFailed => {
            format!(
                "{} execution `{}` finished as {} message={}",
                execution_update_phase_label(phase),
                execution.execution_id,
                status,
                truncate_inline(&execution.result_message, 180)
            )
        }
        pb::ExecutionUpdatePhase::ExecutionBackgrounded => {
            format!(
                "{} execution `{}`",
                execution_update_phase_label(phase),
                execution.execution_id
            )
        }
        pb::ExecutionUpdatePhase::ExecutionRejected => format!(
            "{} execution `{}` message={}",
            execution_update_phase_label(phase),
            execution.execution_id,
            truncate_inline(&execution.result_message, 180)
        ),
        _ => format!(
            "execution `{}` finished as {}",
            execution.execution_id, status
        ),
    }
}

pub(super) fn queued_action_output(
    execution: &pb::Execution,
    call_id: Option<&str>,
    background: bool,
) -> String {
    let status = pb::ExecutionStatus::try_from(execution.status)
        .map(execution_status_label)
        .unwrap_or("unknown");
    let call_suffix = call_id
        .map(|value| format!(" call_id={value}"))
        .unwrap_or_default();
    let mode_suffix = if background { " background=true" } else { "" };

    format!(
        "submitted action `{}` as {} ({status}){}{}",
        execution.action_id, execution.execution_id, call_suffix, mode_suffix
    )
}

fn outcome_phase_for_commit(
    background: bool,
    succeeded: bool,
) -> (
    pb::ExecutionUpdateKind,
    pb::ExecutionUpdatePhase,
    CommitTurnPolicy,
) {
    let (kind, phase) = if succeeded {
        (
            pb::ExecutionUpdateKind::ExecutionSucceeded,
            pb::ExecutionUpdatePhase::ExecutionSucceeded,
        )
    } else {
        (
            pb::ExecutionUpdateKind::ExecutionFailed,
            pb::ExecutionUpdatePhase::ExecutionFailed,
        )
    };
    let policy = if background {
        CommitTurnPolicy::DeferUntilFutureTrigger
    } else {
        CommitTurnPolicy::ResumeNow
    };
    (kind, phase, policy)
}

fn background_requested_from_args_json(args_json: &str) -> Result<bool, String> {
    let value: serde_json::Value = serde_json::from_str(args_json)
        .map_err(|error| format!("failed to parse action args: {error}"))?;
    let Some(object) = value.as_object() else {
        return Err("action arguments must be a JSON object".to_string());
    };
    match object.get("background") {
        None => Ok(false),
        Some(serde_json::Value::Bool(background)) => Ok(*background),
        Some(_) => Err("`background` must be a boolean when provided".to_string()),
    }
}

fn action_result_succeeded(result: &CapabilityActionResult) -> bool {
    result.outcome.is_ok()
}

fn serialize_action_result_message(result: &CapabilityActionResult) -> String {
    let payload = match &result.outcome {
        Ok(success) => json!({
            "ok": true,
            "data": success.payload,
            "execution_time_ms": result.execution_time_ms,
        }),
        Err(ActionError::InputError(error)) => json!({
            "ok": false,
            "error": {
                "kind": "input_error",
                "code": error.code,
                "message": error.message,
                "details": error.details,
            },
            "execution_time_ms": result.execution_time_ms,
        }),
        Err(ActionError::RuntimeError(error)) => json!({
            "ok": false,
            "error": {
                "kind": "runtime_error",
                "code": error.code,
                "message": error.message,
                "details": error.details,
            },
            "execution_time_ms": result.execution_time_ms,
        }),
    };
    payload.to_string()
}

fn truncate_inline(value: &str, max_chars: usize) -> String {
    let value = value.replace('\n', "\\n");
    if value.chars().count() <= max_chars {
        return value;
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use tokio::sync::{broadcast, mpsc};
    use tokio::time::Instant;

    use super::{
        CommitTurnPolicy, QueuedExecutionOutcome, background_expired_submissions,
        handle_capability_domain_action_committed, queue_executions,
    };
    use crate::agent::ActionInvocation;
    use crate::capability_domain::{
        CapabilityDomainActorHandle, CapabilityDomainCommittedAction,
        CapabilityDomainCommittedExecution, build_default_capability_domain_registry,
        spawn_capability_domain_actor,
    };
    use crate::runtime::Runtime;
    use crate::session::state::{
        ExecutionRuntimeState, ExecutionSubmissionExecution, ExecutionSubmissionState,
        ExecutionSubmissionStatus,
    };
    use crate::session::{SessionCommand, SessionState};
    use crate::util::{default_agent_profile, default_user_profile};
    use fathom_capability_domain::{
        CapabilityActionKey, CapabilityActionResult, CapabilityDomainSessionContext,
    };
    use fathom_protocol::pb;
    use serde_json::json;

    fn test_state() -> SessionState {
        let user_id = "user-a".to_string();
        let registry = build_default_capability_domain_registry(
            &std::env::current_dir().expect("current directory for registry"),
        );
        SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            registry
                .installed_capability_domain_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
        )
    }

    fn shell_handle(
        runtime: &Runtime,
        state: &SessionState,
    ) -> (
        HashMap<String, CapabilityDomainActorHandle>,
        mpsc::Receiver<SessionCommand>,
    ) {
        let (session_command_tx, session_command_rx) = mpsc::channel::<SessionCommand>(16);
        let shell_instance = runtime
            .capability_domain_registry()
            .domain_factory("shell")
            .expect("shell factory")
            .create_instance(CapabilityDomainSessionContext {
                session_id: state.session_id.clone(),
            });
        let shell_handle =
            spawn_capability_domain_actor("shell".to_string(), shell_instance, session_command_tx);
        (
            HashMap::from([("shell".to_string(), shell_handle)]),
            session_command_rx,
        )
    }

    #[test]
    fn queue_executions_reject_invalid_background_hint_and_enqueue_execution_rejected_trigger() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, _) = broadcast::channel(16);
        let mut state = test_state();
        let capability_domain_handles = HashMap::new();

        let queued = queue_executions(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            vec![ActionInvocation {
                action_id: "filesystem__list".to_string(),
                args_json: r#"{"path":".","background":"yes"}"#.to_string(),
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            }],
        )
        .pop()
        .expect("queued execution");

        assert!(matches!(queued.outcome, QueuedExecutionOutcome::Rejected));
        assert!(!state.has_blocking_submissions());

        let trigger = state
            .trigger_queue
            .back()
            .expect("execution_rejected trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(update.execution_id, queued.execution.execution_id);
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::ExecutionRejected
        );
    }

    #[tokio::test]
    async fn queue_executions_background_acceptance_backgrounds_without_blocking() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, _) = broadcast::channel(16);
        let mut state = test_state();
        let (capability_domain_handles, _session_command_rx) = shell_handle(&runtime, &state);

        let queued = queue_executions(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            vec![ActionInvocation {
                action_id: "shell__run".to_string(),
                args_json: r#"{"command":"pwd","background":true}"#.to_string(),
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            }],
        )
        .pop()
        .expect("queued execution");

        assert!(matches!(
            queued.outcome,
            QueuedExecutionOutcome::BackgroundAccepted
        ));
        assert!(!state.has_blocking_submissions());
        assert!(
            state
                .execution_runtimes
                .contains_key(&queued.execution.execution_id)
        );
        assert_eq!(
            state
                .execution_submissions
                .values()
                .next()
                .expect("submission")
                .status,
            ExecutionSubmissionStatus::RunningBackground
        );
    }

    #[test]
    fn background_expired_submissions_moves_running_foreground_submission_to_background() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let execution_id = "execution-1".to_string();
        let submission_id = "execution-submission-1".to_string();

        state.executions.insert(
            execution_id.clone(),
            pb::Execution {
                execution_id: execution_id.clone(),
                session_id: state.session_id.clone(),
                action_id: "shell__run".to_string(),
                args_json: r#"{"command":"pwd"}"#.to_string(),
                status: pb::ExecutionStatus::Running as i32,
                result_message: String::new(),
                created_at_unix_ms: 100,
                updated_at_unix_ms: 110,
            },
        );
        state
            .foreground_submission_ids
            .insert(submission_id.clone());
        state.execution_runtimes.insert(
            execution_id.clone(),
            ExecutionRuntimeState {
                submission_id: submission_id.clone(),
                background_requested: false,
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            },
        );
        state.execution_submissions.insert(
            submission_id.clone(),
            ExecutionSubmissionState {
                capability_domain_id: "shell".to_string(),
                executions: vec![ExecutionSubmissionExecution {
                    execution_id: execution_id.clone(),
                    action_key: CapabilityActionKey(0),
                }],
                status: ExecutionSubmissionStatus::RunningForeground,
                foreground_wait_deadline: Some(Instant::now()),
            },
        );

        assert!(background_expired_submissions(
            &runtime, &mut state, &events_tx
        ));
        assert!(!state.has_blocking_submissions());
        assert_eq!(
            state
                .execution_submissions
                .get(&submission_id)
                .expect("submission")
                .status,
            ExecutionSubmissionStatus::RunningBackground
        );
        assert!(
            state
                .execution_runtimes
                .get(&execution_id)
                .expect("execution runtime")
                .background_requested
        );
        let trigger = state
            .trigger_queue
            .back()
            .expect("execution_backgrounded trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::ExecutionBackgrounded
        );
        let execution_update =
            collect_execution_update_event(&mut events_rx).expect("execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::ExecutionBackgrounded as i32
        );
    }

    #[test]
    fn background_expired_submissions_keeps_queued_submission_state_queued() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, _) = broadcast::channel(16);
        let mut state = test_state();
        let execution_id = "execution-1".to_string();
        let submission_id = "execution-submission-1".to_string();

        state.executions.insert(
            execution_id.clone(),
            pb::Execution {
                execution_id: execution_id.clone(),
                session_id: state.session_id.clone(),
                action_id: "shell__run".to_string(),
                args_json: r#"{"command":"pwd"}"#.to_string(),
                status: pb::ExecutionStatus::Pending as i32,
                result_message: String::new(),
                created_at_unix_ms: 100,
                updated_at_unix_ms: 110,
            },
        );
        state
            .foreground_submission_ids
            .insert(submission_id.clone());
        state.execution_runtimes.insert(
            execution_id.clone(),
            ExecutionRuntimeState {
                submission_id: submission_id.clone(),
                background_requested: false,
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            },
        );
        state.execution_submissions.insert(
            submission_id.clone(),
            ExecutionSubmissionState {
                capability_domain_id: "shell".to_string(),
                executions: vec![ExecutionSubmissionExecution {
                    execution_id: execution_id.clone(),
                    action_key: CapabilityActionKey(0),
                }],
                status: ExecutionSubmissionStatus::Queued,
                foreground_wait_deadline: Some(Instant::now()),
            },
        );

        assert!(background_expired_submissions(
            &runtime, &mut state, &events_tx
        ));
        assert!(!state.has_blocking_submissions());
        assert_eq!(
            state
                .execution_submissions
                .get(&submission_id)
                .expect("submission")
                .status,
            ExecutionSubmissionStatus::Queued
        );
        assert!(
            state
                .execution_runtimes
                .get(&execution_id)
                .expect("execution runtime")
                .background_requested
        );
    }

    #[tokio::test]
    async fn queued_foreground_submission_blocks_until_committed() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, _) = broadcast::channel(16);
        let mut state = test_state();
        let (capability_domain_handles, _session_command_rx) = shell_handle(&runtime, &state);

        state.active_submission_ids_by_domain.insert(
            "shell".to_string(),
            "execution-submission-active".to_string(),
        );

        let queued = queue_executions(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            vec![ActionInvocation {
                action_id: "shell__run".to_string(),
                args_json: r#"{"command":"pwd"}"#.to_string(),
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            }],
        )
        .pop()
        .expect("queued execution");

        assert!(matches!(
            queued.outcome,
            QueuedExecutionOutcome::ForegroundAccepted
        ));
        assert!(state.has_blocking_submissions());
        assert_eq!(
            state
                .execution_submissions
                .values()
                .next()
                .expect("submission")
                .status,
            ExecutionSubmissionStatus::Queued
        );
    }

    #[test]
    fn foreground_submission_commit_resumes_agent_and_emits_execution_succeeded_trigger() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let capability_domain_handles = HashMap::new();
        let execution_id = "execution-1".to_string();
        let submission_id = "execution-submission-1".to_string();

        state.executions.insert(
            execution_id.clone(),
            pb::Execution {
                execution_id: execution_id.clone(),
                session_id: state.session_id.clone(),
                action_id: "filesystem__list".to_string(),
                args_json: r#"{"path":"."}"#.to_string(),
                status: pb::ExecutionStatus::Running as i32,
                result_message: String::new(),
                created_at_unix_ms: 100,
                updated_at_unix_ms: 110,
            },
        );
        state
            .foreground_submission_ids
            .insert(submission_id.clone());
        state.execution_runtimes.insert(
            execution_id.clone(),
            ExecutionRuntimeState {
                submission_id: submission_id.clone(),
                background_requested: false,
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            },
        );
        state.execution_submissions.insert(
            submission_id.clone(),
            ExecutionSubmissionState {
                capability_domain_id: "filesystem".to_string(),
                executions: vec![ExecutionSubmissionExecution {
                    execution_id: execution_id.clone(),
                    action_key: CapabilityActionKey(0),
                }],
                status: ExecutionSubmissionStatus::RunningForeground,
                foreground_wait_deadline: None,
            },
        );
        state
            .active_submission_ids_by_domain
            .insert("filesystem".to_string(), submission_id.clone());

        let policy = handle_capability_domain_action_committed(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            CapabilityDomainCommittedAction {
                submission_id,
                capability_domain_id: "filesystem".to_string(),
                executions: vec![CapabilityDomainCommittedExecution {
                    execution_id: execution_id.clone(),
                    result: CapabilityActionResult::success(json!({"entries":["Cargo.toml"]}), 0),
                }],
            },
        );

        assert!(matches!(policy, CommitTurnPolicy::ResumeNow));
        let trigger = state
            .trigger_queue
            .back()
            .expect("execution_succeeded trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::ExecutionSucceeded
        );
        let execution_update =
            collect_execution_update_event(&mut events_rx).expect("execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::ExecutionSucceeded as i32
        );
    }

    #[test]
    fn background_submission_commit_defers_agent_wakeup_and_emits_execution_succeeded_trigger() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let capability_domain_handles = HashMap::new();
        let execution_id = "execution-2".to_string();
        let submission_id = "execution-submission-2".to_string();

        state.executions.insert(
            execution_id.clone(),
            pb::Execution {
                execution_id: execution_id.clone(),
                session_id: state.session_id.clone(),
                action_id: "shell__run".to_string(),
                args_json: r#"{"command":"pwd","background":true}"#.to_string(),
                status: pb::ExecutionStatus::Running as i32,
                result_message: String::new(),
                created_at_unix_ms: 100,
                updated_at_unix_ms: 110,
            },
        );
        state.execution_runtimes.insert(
            execution_id.clone(),
            ExecutionRuntimeState {
                submission_id: submission_id.clone(),
                background_requested: true,
                call_key: "call-key-2".to_string(),
                call_id: Some("call-id-2".to_string()),
            },
        );
        state.execution_submissions.insert(
            submission_id.clone(),
            ExecutionSubmissionState {
                capability_domain_id: "shell".to_string(),
                executions: vec![ExecutionSubmissionExecution {
                    execution_id: execution_id.clone(),
                    action_key: CapabilityActionKey(0),
                }],
                status: ExecutionSubmissionStatus::RunningBackground,
                foreground_wait_deadline: None,
            },
        );
        state
            .active_submission_ids_by_domain
            .insert("shell".to_string(), submission_id.clone());

        let policy = handle_capability_domain_action_committed(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            CapabilityDomainCommittedAction {
                submission_id,
                capability_domain_id: "shell".to_string(),
                executions: vec![CapabilityDomainCommittedExecution {
                    execution_id: execution_id.clone(),
                    result: CapabilityActionResult::success(json!({"stdout":"/tmp"}), 0),
                }],
            },
        );

        assert!(matches!(policy, CommitTurnPolicy::DeferUntilFutureTrigger));
        let trigger = state
            .trigger_queue
            .back()
            .expect("execution_succeeded trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::ExecutionSucceeded
        );
        let execution_update =
            collect_execution_update_event(&mut events_rx).expect("execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::ExecutionSucceeded as i32
        );
    }

    fn collect_execution_update_event(
        events_rx: &mut broadcast::Receiver<pb::SessionEvent>,
    ) -> Option<pb::ExecutionUpdateEvent> {
        while let Ok(event) = events_rx.try_recv() {
            if let Some(pb::session_event::Kind::ExecutionUpdate(item)) = event.kind {
                return Some(item);
            }
        }
        None
    }
}
