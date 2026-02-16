use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tonic::Status;

use crate::history;
use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::{SessionCommand, SessionState};
use crate::session::task_context::TaskExecutionContext;
use crate::session::task_tools::{
    execute_task_tool, extract_send_message_content, should_enqueue_task_done_trigger,
};
use crate::util::{now_unix_ms, task_status_label};

use super::events::{emit_event, enqueue_trigger};

pub(super) fn queue_task(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    tool_name: String,
    args_json: String,
) -> pb::Task {
    let task_id = runtime.next_task_id();
    let now = now_unix_ms();
    let should_run_now = state.running_task_ids.len() < runtime.task_capacity();
    let status = if should_run_now {
        pb::TaskStatus::Running
    } else {
        pb::TaskStatus::Pending
    };

    let task = pb::Task {
        task_id: task_id.clone(),
        session_id: state.session_id.clone(),
        tool_name: tool_name.clone(),
        args_json,
        status: status as i32,
        result_message: String::new(),
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    };
    state.tasks.insert(task_id.clone(), task.clone());

    if should_run_now {
        state.running_task_ids.insert(task_id.clone());
        let execution_context = TaskExecutionContext::from_state(state);
        history::append_task_started_history(state, &task);
        spawn_task_execution(
            runtime,
            command_tx.clone(),
            task_id,
            tool_name,
            task.args_json.clone(),
            execution_context,
        );
    } else {
        state.pending_task_ids.push_back(task_id);
    }

    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::TaskStateChanged(pb::TaskStateChangedEvent {
            task: Some(task.clone()),
        }),
    );

    task
}

pub(super) fn cancel_task(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
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

    if status == pb::TaskStatus::Pending {
        state
            .pending_task_ids
            .retain(|candidate| candidate != task_id);
    } else if status == pb::TaskStatus::Running {
        state.running_task_ids.remove(task_id);
    }

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

    maybe_start_pending_tasks(runtime, state, command_tx, events_tx);

    Ok(pb::CancelTaskResponse {
        canceled: true,
        task: Some(task_snapshot),
    })
}

pub(super) fn handle_finished_task(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    task_id: &str,
    succeeded: bool,
    message: String,
) {
    let Some(task) = state.tasks.get_mut(task_id) else {
        return;
    };
    let status = pb::TaskStatus::try_from(task.status).unwrap_or(pb::TaskStatus::Unspecified);
    if status == pb::TaskStatus::Canceled {
        return;
    }
    if !matches!(status, pb::TaskStatus::Running | pb::TaskStatus::Pending) {
        return;
    }

    state.running_task_ids.remove(task_id);

    task.status = if succeeded {
        pb::TaskStatus::Succeeded as i32
    } else {
        pb::TaskStatus::Failed as i32
    };
    task.result_message = message.clone();
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

    if let Some(content) = extract_send_message_content(&task_snapshot) {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::AssistantOutput(pb::AssistantOutputEvent {
                content: content.clone(),
            }),
        );
        history::append_assistant_output_history(state, &content);
    }

    if should_enqueue_task_done_trigger(&task_snapshot.tool_name) {
        let trigger = pb::Trigger {
            trigger_id: runtime.next_trigger_id(),
            created_at_unix_ms: now_unix_ms(),
            kind: Some(pb::trigger::Kind::TaskDone(pb::TaskDoneTrigger {
                task_id: task_snapshot.task_id,
                status: task_snapshot.status,
                result_message: task_snapshot.result_message,
            })),
        };
        enqueue_trigger(state, events_tx, trigger);
    }

    maybe_start_pending_tasks(runtime, state, command_tx, events_tx);
}

fn maybe_start_pending_tasks(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
) {
    while state.running_task_ids.len() < runtime.task_capacity() {
        let Some(task_id) = state.pending_task_ids.pop_front() else {
            break;
        };
        let (tool_name, args_json, task_snapshot) = {
            let Some(task) = state.tasks.get_mut(&task_id) else {
                continue;
            };
            if task.status != pb::TaskStatus::Pending as i32 {
                continue;
            }

            task.status = pb::TaskStatus::Running as i32;
            task.updated_at_unix_ms = now_unix_ms();
            (task.tool_name.clone(), task.args_json.clone(), task.clone())
        };
        let execution_context = TaskExecutionContext::from_state(state);

        state.running_task_ids.insert(task_id.clone());
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TaskStateChanged(pb::TaskStateChangedEvent {
                task: Some(task_snapshot.clone()),
            }),
        );
        history::append_task_started_history(state, &task_snapshot);
        spawn_task_execution(
            runtime,
            command_tx.clone(),
            task_id,
            tool_name,
            args_json,
            execution_context,
        );
    }
}

fn spawn_task_execution(
    runtime: &Runtime,
    command_tx: mpsc::Sender<SessionCommand>,
    task_id: String,
    tool_name: String,
    args_json: String,
    execution_context: TaskExecutionContext,
) {
    let runtime = runtime.clone();
    tokio::spawn(async move {
        let (succeeded, message) = if let Some(outcome) =
            execute_task_tool(&runtime, &execution_context, &tool_name, &args_json).await
        {
            (outcome.succeeded, outcome.message)
        } else {
            tokio::time::sleep(Duration::from_millis(runtime.task_runtime_ms())).await;
            (true, format!("tool `{tool_name}` completed"))
        };

        let _ = command_tx
            .send(SessionCommand::TaskFinished {
                task_id,
                succeeded,
                message,
            })
            .await;
    });
}

pub(super) fn queued_tool_output(task: &pb::Task, call_id: Option<&str>) -> String {
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");
    let call_suffix = call_id
        .map(|value| format!(" call_id={value}"))
        .unwrap_or_default();

    format!(
        "queued tool `{}` as {} ({status}){}",
        task.tool_name, task.task_id, call_suffix
    )
}
