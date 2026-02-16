use std::collections::HashMap;

use tokio::sync::broadcast;
use tonic::Status;

use crate::environment::{
    EnvironmentActionSubmission, EnvironmentActorHandle, EnvironmentCommittedAction,
    EnvironmentRegistry,
};
use crate::history;
use crate::history::build_payload_preview;
use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::{InFlightActionState, SessionState};
use crate::session::task_context::TaskExecutionContext;
use crate::util::{now_unix_ms, task_status_label};

use super::events::{emit_event, enqueue_trigger};

pub(super) fn queue_task(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    environment_handles: &HashMap<String, EnvironmentActorHandle>,
    action_id: String,
    args_json: String,
) -> pb::Task {
    let task_id = runtime.next_task_id();
    let now = now_unix_ms();

    let resolved = EnvironmentRegistry::resolve(&action_id);

    let mut task = pb::Task {
        task_id: task_id.clone(),
        session_id: state.session_id.clone(),
        action_id: action_id.clone(),
        args_json: args_json.clone(),
        status: pb::TaskStatus::Running as i32,
        result_message: String::new(),
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    };

    if let Some(resolved_action) = resolved {
        if !state
            .engaged_environment_ids
            .contains(&resolved_action.environment_id)
        {
            task.status = pb::TaskStatus::Failed as i32;
            task.result_message = format!(
                "environment `{}` is not engaged for this session",
                resolved_action.environment_id
            );
        } else if let Some(handle) = environment_handles.get(&resolved_action.environment_id) {
            let env_seq = state.allocate_environment_seq(&resolved_action.environment_id);
            let execution_context = TaskExecutionContext::from_state(state);

            let args_preview =
                build_payload_preview(&task.args_json, format!("task://{}/args", task.task_id))
                    .preview;

            state.in_flight_actions.insert(
                task_id.clone(),
                InFlightActionState {
                    task_id: task_id.clone(),
                    canonical_action_id: resolved_action.canonical_action_id.clone(),
                    environment_id: resolved_action.environment_id.clone(),
                    action_name: resolved_action.action_name.clone(),
                    env_seq,
                    status: "executing".to_string(),
                    submitted_at_unix_ms: now,
                    args_preview,
                },
            );

            let submission = EnvironmentActionSubmission {
                task_id: task_id.clone(),
                env_seq,
                resolved_action,
                args_json: task.args_json.clone(),
                execution_context,
            };

            let handle = handle.clone();
            tokio::spawn(async move {
                handle.submit(submission).await;
            });
        } else {
            task.status = pb::TaskStatus::Failed as i32;
            task.result_message = format!(
                "environment runtime `{}` is unavailable",
                resolved_action.environment_id
            );
        }
    } else {
        task.status = pb::TaskStatus::Failed as i32;
        task.result_message = format!("unknown action `{action_id}`");
    }

    state.tasks.insert(task_id.clone(), task.clone());

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::TaskStateChanged(pb::TaskStateChangedEvent {
            task: Some(task.clone()),
        }),
    );

    history::append_task_started_history(state, &task);

    if task.status == pb::TaskStatus::Failed as i32 {
        history::append_task_finished_history(state, &task);
        enqueue_task_done_trigger(runtime, state, events_tx, &task);
    }

    task
}

pub(super) fn cancel_task(
    _runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    task_id: &str,
) -> Result<pb::CancelTaskResponse, Status> {
    let Some(task) = state.tasks.get_mut(task_id) else {
        return Err(Status::not_found("task not found"));
    };

    let status = pb::TaskStatus::try_from(task.status).unwrap_or(pb::TaskStatus::Unspecified);
    let is_terminal = matches!(
        status,
        pb::TaskStatus::Succeeded | pb::TaskStatus::Failed | pb::TaskStatus::Canceled
    );
    if is_terminal {
        return Ok(pb::CancelTaskResponse {
            canceled: false,
            task: Some(task.clone()),
        });
    }

    state.in_flight_actions.remove(task_id);

    task.status = pb::TaskStatus::Canceled as i32;
    task.result_message = "canceled by request".to_string();
    task.updated_at_unix_ms = now_unix_ms();
    let task_snapshot = task.clone();

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::TaskStateChanged(pb::TaskStateChangedEvent {
            task: Some(task_snapshot.clone()),
        }),
    );

    Ok(pb::CancelTaskResponse {
        canceled: true,
        task: Some(task_snapshot),
    })
}

pub(super) fn handle_environment_action_committed(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    committed: EnvironmentCommittedAction,
) {
    if !state
        .engaged_environment_ids
        .contains(&committed.environment_id)
    {
        return;
    }

    state
        .environment_snapshots
        .insert(committed.environment_id.clone(), committed.state_snapshot);

    let Some(task) = state.tasks.get_mut(&committed.task_id) else {
        return;
    };

    state.in_flight_actions.remove(&committed.task_id);

    let status = pb::TaskStatus::try_from(task.status).unwrap_or(pb::TaskStatus::Unspecified);
    if status == pb::TaskStatus::Canceled {
        return;
    }
    if !matches!(status, pb::TaskStatus::Running | pb::TaskStatus::Pending) {
        return;
    }

    task.status = if committed.succeeded {
        pb::TaskStatus::Succeeded as i32
    } else {
        pb::TaskStatus::Failed as i32
    };
    task.result_message = committed.message;
    task.updated_at_unix_ms = now_unix_ms();
    let task_snapshot = task.clone();

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::TaskStateChanged(pb::TaskStateChangedEvent {
            task: Some(task_snapshot.clone()),
        }),
    );
    history::append_task_finished_history(state, &task_snapshot);

    enqueue_task_done_trigger(runtime, state, events_tx, &task_snapshot);
}

fn enqueue_task_done_trigger(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    task: &pb::Task,
) {
    let trigger = pb::Trigger {
        trigger_id: runtime.next_trigger_id(),
        created_at_unix_ms: now_unix_ms(),
        kind: Some(pb::trigger::Kind::TaskDone(pb::TaskDoneTrigger {
            task_id: task.task_id.clone(),
            status: task.status,
            result_message: task.result_message.clone(),
        })),
    };
    enqueue_trigger(state, events_tx, trigger);
}

pub(super) fn queued_action_output(task: &pb::Task, call_id: Option<&str>) -> String {
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");
    let call_suffix = call_id
        .map(|value| format!(" call_id={value}"))
        .unwrap_or_default();

    format!(
        "queued action `{}` as {} ({status}){}",
        task.action_id, task.task_id, call_suffix
    )
}
