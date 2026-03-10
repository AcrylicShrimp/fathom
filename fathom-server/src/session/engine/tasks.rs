use std::collections::HashMap;

use tokio::sync::broadcast;
use tonic::Status;

use crate::agent::ActionInvocation;
use crate::capability_domain::{
    CapabilityDomainActionSubmission, CapabilityDomainActorHandle, CapabilityDomainCommittedAction,
    CapabilityDomainRegistry, RequestedExecutionMode, requested_execution_mode_from_args_json,
};
use crate::history;
use crate::runtime::Runtime;
use crate::session::diagnostics::execution_to_json;
use crate::session::execution_context::ExecutionContext;
use crate::session::payload_lookup::resolve_from_execution;
use crate::session::state::{ActiveExecutionState, SessionState};
use crate::util::now_unix_ms;
use fathom_capability_domain::ActionModeSupport;
use fathom_protocol::pb;
use fathom_protocol::{execution_status_label, execution_update_phase_label};

use super::events::{emit_event, emit_execution_update_event, enqueue_trigger};

pub(super) struct QueuedExecution {
    pub(super) execution: pb::Execution,
    pub(super) outcome: QueuedExecutionOutcome,
}

pub(super) enum QueuedExecutionOutcome {
    AwaitAccepted,
    DetachedAccepted,
    Rejected,
}

pub(super) enum CommitTurnPolicy {
    ResumeNow,
    DeferUntilFutureTrigger,
}

pub(super) fn queue_execution(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
    action_invocation: ActionInvocation,
) -> QueuedExecution {
    let ActionInvocation {
        action_id,
        args_json,
        call_key,
        call_id,
    } = action_invocation;
    let execution_id = runtime.next_execution_id();
    let now = now_unix_ms();
    let requested_mode = requested_execution_mode_from_args_json(&args_json);

    let mut execution = pb::Execution {
        execution_id: execution_id.clone(),
        session_id: state.session_id.clone(),
        action_id: action_id.clone(),
        args_json: args_json.clone(),
        status: pb::ExecutionStatus::Running as i32,
        result_message: String::new(),
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    };
    let mut outcome = QueuedExecutionOutcome::Rejected;

    match requested_mode {
        Ok(requested_mode) => {
            let resolved = CapabilityDomainRegistry::resolve(&action_id);
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
                } else if requested_mode == RequestedExecutionMode::Detach
                    && resolved_action.mode_support != ActionModeSupport::AwaitOrDetach
                {
                    execution.status = pb::ExecutionStatus::Failed as i32;
                    execution.result_message = format!(
                        "detach is not allowed for `{}`; use await",
                        resolved_action.canonical_action_id
                    );
                } else if let Some(handle) =
                    capability_domain_handles.get(&resolved_action.capability_domain_id)
                {
                    let env_seq =
                        state.allocate_capability_domain_seq(&resolved_action.capability_domain_id);
                    let execution_context = ExecutionContext::from_state(state);

                    state.active_executions.insert(
                        execution_id.clone(),
                        ActiveExecutionState {
                            requested_mode,
                            call_key: call_key.clone(),
                            call_id: call_id.clone(),
                        },
                    );

                    if requested_mode == RequestedExecutionMode::Await {
                        state.in_flight_actions.insert(execution_id.clone());
                        outcome = QueuedExecutionOutcome::AwaitAccepted;
                    } else {
                        outcome = QueuedExecutionOutcome::DetachedAccepted;
                    }

                    let submission = CapabilityDomainActionSubmission {
                        execution_id: execution_id.clone(),
                        env_seq,
                        resolved_action,
                        args_json: execution.args_json.clone(),
                        execution_context,
                    };

                    let handle = handle.clone();
                    tokio::spawn(async move {
                        handle.submit(submission).await;
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

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::ExecutionStateChanged(pb::ExecutionStateChangedEvent {
            execution: Some(execution.clone()),
        }),
    );

    history::append_execution_requested_history(state, &execution);
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "execution.started",
            "session_id": state.session_id,
            "execution": execution_to_json(&execution),
        }),
    );

    if matches!(outcome, QueuedExecutionOutcome::Rejected) {
        runtime.diagnostics().append_session_record(
            &state.session_id,
            serde_json::json!({
                "ts_unix_ms": now_unix_ms(),
                "event": "execution.rejected",
                "session_id": state.session_id,
                "execution": execution_to_json(&execution),
            }),
        );
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
    } else if matches!(outcome, QueuedExecutionOutcome::DetachedAccepted) {
        enqueue_execution_update_trigger(
            runtime,
            state,
            events_tx,
            build_execution_update_trigger(
                runtime,
                &execution.execution_id,
                &execution.action_id,
                pb::ExecutionUpdateKind::ExecutionDetached,
                String::new(),
                String::new(),
            ),
        );
    }

    QueuedExecution { execution, outcome }
}

pub(super) fn cancel_execution(
    _runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
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

    state.in_flight_actions.remove(execution_id);
    state.active_executions.remove(execution_id);

    let execution = state
        .executions
        .get_mut(execution_id)
        .expect("execution must exist after terminality check");
    execution.status = pb::ExecutionStatus::Canceled as i32;
    execution.result_message = "canceled by request".to_string();
    execution.updated_at_unix_ms = now_unix_ms();
    let execution_snapshot = execution.clone();

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::ExecutionStateChanged(pb::ExecutionStateChangedEvent {
            execution: Some(execution_snapshot.clone()),
        }),
    );

    Ok(pb::CancelExecutionResponse {
        canceled: true,
        execution: Some(execution_snapshot),
    })
}

pub(super) fn handle_capability_domain_action_committed(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    committed: CapabilityDomainCommittedAction,
) -> CommitTurnPolicy {
    if !state
        .engaged_capability_domain_ids
        .contains(&committed.capability_domain_id)
    {
        return CommitTurnPolicy::DeferUntilFutureTrigger;
    }

    state.capability_domain_snapshots.insert(
        committed.capability_domain_id.clone(),
        committed.state_snapshot,
    );

    state.in_flight_actions.remove(&committed.execution_id);
    let execution_state = state.active_executions.remove(&committed.execution_id);
    let Some(execution) = state.executions.get_mut(&committed.execution_id) else {
        return CommitTurnPolicy::DeferUntilFutureTrigger;
    };

    let status =
        pb::ExecutionStatus::try_from(execution.status).unwrap_or(pb::ExecutionStatus::Unspecified);
    if status == pb::ExecutionStatus::Canceled {
        return CommitTurnPolicy::DeferUntilFutureTrigger;
    }
    if !matches!(
        status,
        pb::ExecutionStatus::Running | pb::ExecutionStatus::Pending
    ) {
        return CommitTurnPolicy::DeferUntilFutureTrigger;
    }

    execution.status = if committed.succeeded {
        pb::ExecutionStatus::Succeeded as i32
    } else {
        pb::ExecutionStatus::Failed as i32
    };
    execution.result_message = committed.message;
    execution.updated_at_unix_ms = now_unix_ms();
    let execution_snapshot = execution.clone();

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::ExecutionStateChanged(pb::ExecutionStateChangedEvent {
            execution: Some(execution_snapshot.clone()),
        }),
    );
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

    let requested_mode = execution_state
        .as_ref()
        .map(|state| state.requested_mode)
        .or_else(|| requested_execution_mode_from_args_json(&execution_snapshot.args_json).ok())
        .unwrap_or(RequestedExecutionMode::Await);
    let (trigger_kind, phase, policy) =
        outcome_phase_for_commit(requested_mode, committed.succeeded);
    let detail = settled_execution_output(&execution_snapshot, phase);

    emit_execution_update_event(
        events_tx,
        &state.session_id,
        phase,
        execution_state
            .as_ref()
            .map(|item| item.call_key.clone())
            .unwrap_or_default(),
        execution_state
            .as_ref()
            .and_then(|item| item.call_id.clone()),
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
            if committed.succeeded {
                String::new()
            } else {
                execution_snapshot.result_message.clone()
            },
            execution_snapshot.result_message.clone(),
        ),
    );
    policy
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
        pb::ExecutionUpdatePhase::AwaitedExecutionSucceeded
        | pb::ExecutionUpdatePhase::DetachedExecutionSucceeded => format!(
            "{} execution `{}` finished as {}",
            execution_update_phase_label(phase),
            execution.execution_id,
            status
        ),
        pb::ExecutionUpdatePhase::AwaitedExecutionFailed
        | pb::ExecutionUpdatePhase::DetachedExecutionFailed => {
            format!(
                "{} execution `{}` finished as {} message={}",
                execution_update_phase_label(phase),
                execution.execution_id,
                status,
                truncate_inline(&execution.result_message, 180)
            )
        }
        pb::ExecutionUpdatePhase::ExecutionDetached => {
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
    requested_mode: RequestedExecutionMode,
) -> String {
    let status = pb::ExecutionStatus::try_from(execution.status)
        .map(execution_status_label)
        .unwrap_or("unknown");
    let call_suffix = call_id
        .map(|value| format!(" call_id={value}"))
        .unwrap_or_default();
    let mode_suffix = if requested_mode == RequestedExecutionMode::Detach {
        " mode=detach"
    } else {
        ""
    };

    format!(
        "submitted action `{}` as {} ({status}){}{}",
        execution.action_id, execution.execution_id, call_suffix, mode_suffix
    )
}

fn outcome_phase_for_commit(
    requested_mode: RequestedExecutionMode,
    succeeded: bool,
) -> (
    pb::ExecutionUpdateKind,
    pb::ExecutionUpdatePhase,
    CommitTurnPolicy,
) {
    match (requested_mode, succeeded) {
        (RequestedExecutionMode::Await, true) => (
            pb::ExecutionUpdateKind::AwaitedExecutionSucceeded,
            pb::ExecutionUpdatePhase::AwaitedExecutionSucceeded,
            CommitTurnPolicy::ResumeNow,
        ),
        (RequestedExecutionMode::Await, false) => (
            pb::ExecutionUpdateKind::AwaitedExecutionFailed,
            pb::ExecutionUpdatePhase::AwaitedExecutionFailed,
            CommitTurnPolicy::ResumeNow,
        ),
        (RequestedExecutionMode::Detach, true) => (
            pb::ExecutionUpdateKind::DetachedExecutionSucceeded,
            pb::ExecutionUpdatePhase::DetachedExecutionSucceeded,
            CommitTurnPolicy::DeferUntilFutureTrigger,
        ),
        (RequestedExecutionMode::Detach, false) => (
            pb::ExecutionUpdateKind::DetachedExecutionFailed,
            pb::ExecutionUpdatePhase::DetachedExecutionFailed,
            CommitTurnPolicy::DeferUntilFutureTrigger,
        ),
    }
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

    use super::{
        CommitTurnPolicy, QueuedExecutionOutcome, handle_capability_domain_action_committed,
        queue_execution,
    };
    use crate::agent::ActionInvocation;
    use crate::capability_domain::{
        CapabilityDomainCommittedAction, CapabilityDomainRegistry, RequestedExecutionMode,
        spawn_capability_domain_actor,
    };
    use crate::runtime::Runtime;
    use crate::session::state::ActiveExecutionState;
    use crate::session::{SessionCommand, SessionState};
    use crate::util::{default_agent_profile, default_user_profile};
    use fathom_protocol::pb;

    fn test_state() -> SessionState {
        let user_id = "user-a".to_string();
        SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            CapabilityDomainRegistry::default_engaged_capability_domain_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            CapabilityDomainRegistry::initial_capability_domain_snapshots()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        )
    }

    #[test]
    fn queue_execution_rejects_illegal_detach_and_enqueues_execution_rejected_trigger() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, _) = broadcast::channel(16);
        let mut state = test_state();
        let capability_domain_handles = HashMap::new();

        let queued = queue_execution(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            ActionInvocation {
                action_id: "filesystem__list".to_string(),
                args_json: r#"{"path":".","execution_mode":"detach"}"#.to_string(),
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            },
        );

        assert!(matches!(queued.outcome, QueuedExecutionOutcome::Rejected));
        assert!(state.in_flight_actions.is_empty());

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
        assert_eq!(update.action_id, "filesystem__list");
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::ExecutionRejected
        );
    }

    #[tokio::test]
    async fn queue_execution_detach_acceptance_enqueues_execution_detached_without_blocking() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, _) = broadcast::channel(16);
        let mut state = test_state();
        let (session_command_tx, _session_command_rx) = mpsc::channel::<SessionCommand>(16);
        let shell_snapshot = state
            .capability_domain_snapshots
            .get("shell")
            .cloned()
            .expect("shell snapshot");
        let shell_handle = spawn_capability_domain_actor(
            runtime.clone(),
            "shell".to_string(),
            shell_snapshot,
            session_command_tx,
        );
        let capability_domain_handles = HashMap::from([("shell".to_string(), shell_handle)]);

        let queued = queue_execution(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            ActionInvocation {
                action_id: "shell__run".to_string(),
                args_json: r#"{"command":"pwd","execution_mode":"detach"}"#.to_string(),
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            },
        );

        assert!(matches!(
            queued.outcome,
            QueuedExecutionOutcome::DetachedAccepted
        ));
        assert!(state.in_flight_actions.is_empty());
        assert!(
            state
                .active_executions
                .contains_key(&queued.execution.execution_id)
        );

        let trigger = state
            .trigger_queue
            .back()
            .expect("execution_detached trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::ExecutionDetached
        );
    }

    #[test]
    fn awaited_commit_resumes_agent_and_emits_awaited_execution_trigger() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let created_at = 100;
        let updated_at = 110;
        let execution_id = "execution-1".to_string();
        let execution = pb::Execution {
            execution_id: execution_id.clone(),
            session_id: state.session_id.clone(),
            action_id: "filesystem__list".to_string(),
            args_json: r#"{"path":"."}"#.to_string(),
            status: pb::ExecutionStatus::Running as i32,
            result_message: String::new(),
            created_at_unix_ms: created_at,
            updated_at_unix_ms: updated_at,
        };
        state.executions.insert(execution_id.clone(), execution);
        state.in_flight_actions.insert(execution_id.clone());
        state.active_executions.insert(
            execution_id.clone(),
            ActiveExecutionState {
                requested_mode: RequestedExecutionMode::Await,
                call_key: "call-key-1".to_string(),
                call_id: Some("call-id-1".to_string()),
            },
        );

        let committed = CapabilityDomainCommittedAction {
            execution_id: execution_id.clone(),
            capability_domain_id: "filesystem".to_string(),
            succeeded: true,
            message: r#"{"entries":["Cargo.toml"]}"#.to_string(),
            state_snapshot: state
                .capability_domain_snapshots
                .get("filesystem")
                .cloned()
                .expect("filesystem snapshot"),
        };

        let policy =
            handle_capability_domain_action_committed(&runtime, &mut state, &events_tx, committed);

        assert!(matches!(policy, CommitTurnPolicy::ResumeNow));
        let trigger = state
            .trigger_queue
            .back()
            .expect("awaited execution trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::AwaitedExecutionSucceeded
        );
        let execution_update =
            collect_execution_update_event(&mut events_rx).expect("execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::AwaitedExecutionSucceeded as i32
        );
    }

    #[test]
    fn detached_commit_defers_agent_wakeup_and_emits_detached_execution_trigger() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let created_at = 100;
        let updated_at = 110;
        let execution_id = "execution-2".to_string();
        let execution = pb::Execution {
            execution_id: execution_id.clone(),
            session_id: state.session_id.clone(),
            action_id: "shell__run".to_string(),
            args_json: r#"{"command":"pwd","execution_mode":"detach"}"#.to_string(),
            status: pb::ExecutionStatus::Running as i32,
            result_message: String::new(),
            created_at_unix_ms: created_at,
            updated_at_unix_ms: updated_at,
        };
        state.executions.insert(execution_id.clone(), execution);
        state.active_executions.insert(
            execution_id.clone(),
            ActiveExecutionState {
                requested_mode: RequestedExecutionMode::Detach,
                call_key: "call-key-2".to_string(),
                call_id: Some("call-id-2".to_string()),
            },
        );

        let committed = CapabilityDomainCommittedAction {
            execution_id: execution_id.clone(),
            capability_domain_id: "shell".to_string(),
            succeeded: true,
            message: r#"{"stdout":"/tmp"}"#.to_string(),
            state_snapshot: state
                .capability_domain_snapshots
                .get("shell")
                .cloned()
                .expect("shell snapshot"),
        };

        let policy =
            handle_capability_domain_action_committed(&runtime, &mut state, &events_tx, committed);

        assert!(matches!(policy, CommitTurnPolicy::DeferUntilFutureTrigger));
        let trigger = state
            .trigger_queue
            .back()
            .expect("detached execution trigger");
        let pb::trigger::Kind::ExecutionUpdate(update) =
            trigger.kind.as_ref().expect("trigger kind")
        else {
            panic!("expected execution update trigger");
        };
        assert_eq!(
            pb::ExecutionUpdateKind::try_from(update.kind).expect("execution update kind"),
            pb::ExecutionUpdateKind::DetachedExecutionSucceeded
        );
        let execution_update =
            collect_execution_update_event(&mut events_rx).expect("execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::DetachedExecutionSucceeded as i32
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
