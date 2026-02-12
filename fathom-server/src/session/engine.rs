use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::agent::{StreamNote, ToolInvocation};
use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::{SessionCommand, SessionState};
use crate::util::{now_unix_ms, refresh_scope_label, task_status_label};

pub(crate) async fn run_session_actor(
    runtime: Runtime,
    mut state: SessionState,
    command_tx: mpsc::Sender<SessionCommand>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    events_tx: broadcast::Sender<pb::SessionEvent>,
) {
    while let Some(command) = command_rx.recv().await {
        match command {
            SessionCommand::EnqueueTrigger {
                trigger,
                respond_to,
            } => {
                let queue_depth = enqueue_trigger(&mut state, &events_tx, trigger);
                let _ = respond_to.send(Ok(pb::EnqueueTriggerResponse {
                    trigger_id: state
                        .trigger_queue
                        .back()
                        .map(|trigger| trigger.trigger_id.clone())
                        .unwrap_or_default(),
                    queue_depth,
                }));
                process_turns(&runtime, &mut state, &command_tx, &events_tx).await;
            }
            SessionCommand::GetSummary { respond_to } => {
                let _ = respond_to.send(state.to_summary());
            }
            SessionCommand::ListTasks { respond_to } => {
                let mut tasks = state.tasks.values().cloned().collect::<Vec<_>>();
                tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
                let _ = respond_to.send(tasks);
            }
            SessionCommand::CancelTask {
                task_id,
                respond_to,
            } => {
                let response = cancel_task(&runtime, &mut state, &command_tx, &events_tx, &task_id);
                let _ = respond_to.send(response);
            }
            SessionCommand::TaskFinished {
                task_id,
                succeeded,
                message,
            } => {
                handle_finished_task(
                    &runtime,
                    &mut state,
                    &command_tx,
                    &events_tx,
                    &task_id,
                    succeeded,
                    message,
                );
                process_turns(&runtime, &mut state, &command_tx, &events_tx).await;
            }
        }
    }
}

fn enqueue_trigger(
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    trigger: pb::Trigger,
) -> u64 {
    state.trigger_queue.push_back(trigger.clone());
    let queue_depth = state.trigger_queue.len() as u64;
    emit_event(
        events_tx,
        &state.session_id,
        pb::session_event::Kind::TriggerAccepted(pb::TriggerAcceptedEvent {
            trigger: Some(trigger),
            queue_depth,
        }),
    );
    queue_depth
}

async fn process_turns(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
) {
    if state.turn_in_progress {
        return;
    }

    state.turn_in_progress = true;
    while !state.trigger_queue.is_empty() {
        state.turn_seq += 1;
        let turn_id = state.turn_seq;

        let mut turn_triggers = Vec::with_capacity(state.trigger_queue.len());
        while let Some(trigger) = state.trigger_queue.pop_front() {
            turn_triggers.push(trigger);
        }

        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TurnStarted(pb::TurnStartedEvent {
                turn_id,
                trigger_count: turn_triggers.len() as u64,
            }),
        );

        let mut assistant_outputs = Vec::new();
        let mut agent_triggers = Vec::new();

        for trigger in &turn_triggers {
            match trigger.kind.as_ref() {
                Some(pb::trigger::Kind::RefreshProfile(refresh)) => {
                    let refreshed_user_ids = apply_profile_refresh(runtime, state, refresh).await;
                    emit_event(
                        events_tx,
                        &state.session_id,
                        pb::session_event::Kind::ProfileRefreshed(pb::ProfileRefreshedEvent {
                            scope: refresh.scope,
                            refreshed_user_ids,
                            agent_spec_version: state.agent_profile_copy.spec_version,
                        }),
                    );
                    assistant_outputs.push("profile copies refreshed for this session".to_string());
                }
                _ => agent_triggers.push(trigger.clone()),
            }
        }

        if !agent_triggers.is_empty() {
            run_agent_turn(
                runtime,
                state,
                command_tx,
                events_tx,
                turn_id,
                &agent_triggers,
                &mut assistant_outputs,
            )
            .await;
        }

        for output in &assistant_outputs {
            emit_event(
                events_tx,
                &state.session_id,
                pb::session_event::Kind::AssistantOutput(pb::AssistantOutputEvent {
                    content: output.clone(),
                }),
            );
        }

        flush_history(state, &turn_triggers, &assistant_outputs);
        let reason = format!("processed {} trigger(s)", turn_triggers.len());
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TurnEnded(pb::TurnEndedEvent {
                turn_id,
                reason,
                history_size: state.history.len() as u64,
            }),
        );
    }
    state.turn_in_progress = false;
}

async fn run_agent_turn(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    turn_id: u64,
    agent_triggers: &[pb::Trigger],
    assistant_outputs: &mut Vec<String>,
) {
    let snapshot = runtime.build_turn_snapshot(state, turn_id, agent_triggers);
    let orchestrator = runtime.agent_orchestrator();
    let session_id = state.session_id.clone();

    let outcome = orchestrator
        .run_turn(
            &snapshot,
            |note: StreamNote| {
                emit_event(
                    events_tx,
                    &session_id,
                    pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                        phase: note.phase,
                        detail: note.detail,
                        created_at_unix_ms: now_unix_ms(),
                    }),
                );
            },
            |tool_invocation: ToolInvocation| {
                let task = queue_task(
                    runtime,
                    state,
                    command_tx,
                    events_tx,
                    tool_invocation.tool_name,
                    tool_invocation.args_json,
                );
                let status = pb::TaskStatus::try_from(task.status)
                    .map(task_status_label)
                    .unwrap_or("unknown");
                let call_suffix = tool_invocation
                    .call_id
                    .as_ref()
                    .map(|call_id| format!(" call_id={call_id}"))
                    .unwrap_or_default();

                assistant_outputs.push(format!(
                    "queued tool `{}` as {} ({status}){}",
                    task.tool_name, task.task_id, call_suffix
                ));
            },
        )
        .await;

    assistant_outputs.extend(outcome.diagnostics.clone());

    if outcome.failed {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TurnFailure(pb::TurnFailureEvent {
                turn_id,
                reason_code: outcome.failure_code.clone(),
                message: outcome.failure_message.clone(),
            }),
        );
        assistant_outputs.push(format!(
            "turn failed [{}]: {}",
            outcome.failure_code, outcome.failure_message
        ));
        return;
    }

    assistant_outputs.push(format!(
        "agent dispatched {} tool call(s)",
        outcome.tool_call_count
    ));
}

fn queue_task(
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
        spawn_task_completion(runtime, command_tx.clone(), task_id, tool_name);
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

fn cancel_task(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    task_id: &str,
) -> Result<pb::CancelTaskResponse, tonic::Status> {
    let Some(task) = state.tasks.get_mut(task_id) else {
        return Err(tonic::Status::not_found("task not found"));
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

fn handle_finished_task(
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
        let Some(task) = state.tasks.get_mut(&task_id) else {
            continue;
        };
        if task.status != pb::TaskStatus::Pending as i32 {
            continue;
        }

        task.status = pb::TaskStatus::Running as i32;
        task.updated_at_unix_ms = now_unix_ms();
        let tool_name = task.tool_name.clone();
        let task_snapshot = task.clone();

        state.running_task_ids.insert(task_id.clone());
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TaskStateChanged(pb::TaskStateChangedEvent {
                task: Some(task_snapshot),
            }),
        );
        spawn_task_completion(runtime, command_tx.clone(), task_id, tool_name);
    }
}

fn spawn_task_completion(
    runtime: &Runtime,
    command_tx: mpsc::Sender<SessionCommand>,
    task_id: String,
    tool_name: String,
) {
    let runtime_ms = runtime.task_runtime_ms();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(runtime_ms)).await;
        let _ = command_tx
            .send(SessionCommand::TaskFinished {
                task_id,
                succeeded: true,
                message: format!("tool `{tool_name}` completed"),
            })
            .await;
    });
}

async fn apply_profile_refresh(
    runtime: &Runtime,
    state: &mut SessionState,
    refresh: &pb::RefreshProfileTrigger,
) -> Vec<String> {
    let scope = pb::RefreshScope::try_from(refresh.scope).unwrap_or(pb::RefreshScope::All);
    let mut refreshed_user_ids = Vec::new();

    if matches!(scope, pb::RefreshScope::Agent | pb::RefreshScope::All)
        && let Some(profile) = runtime.fetch_agent_profile(&state.agent_id).await
    {
        state.agent_profile_copy = profile;
    }

    if matches!(scope, pb::RefreshScope::User | pb::RefreshScope::All) {
        if scope == pb::RefreshScope::User && !refresh.user_id.trim().is_empty() {
            if let Some(profile) = runtime.fetch_user_profile(&refresh.user_id).await {
                state
                    .participant_user_profiles_copy
                    .insert(refresh.user_id.clone(), profile);
                refreshed_user_ids.push(refresh.user_id.clone());
            }
        } else {
            for user_id in &state.participant_user_ids {
                if let Some(profile) = runtime.fetch_user_profile(user_id).await {
                    state
                        .participant_user_profiles_copy
                        .insert(user_id.clone(), profile);
                    refreshed_user_ids.push(user_id.clone());
                }
            }
        }
    }

    refreshed_user_ids
}

fn flush_history(
    state: &mut SessionState,
    turn_triggers: &[pb::Trigger],
    assistant_outputs: &[String],
) {
    for trigger in turn_triggers {
        state.history.push(format!(
            "{} trigger {}",
            trigger.created_at_unix_ms,
            trigger_to_history_text(trigger)
        ));
    }

    for output in assistant_outputs {
        state
            .history
            .push(format!("{} assistant {}", now_unix_ms(), output));
    }
}

fn trigger_to_history_text(trigger: &pb::Trigger) -> String {
    let Some(kind) = trigger.kind.as_ref() else {
        return "unknown trigger".to_string();
    };
    match kind {
        pb::trigger::Kind::UserMessage(message) => {
            format!("user:{} {}", message.user_id, message.text)
        }
        pb::trigger::Kind::TaskDone(done) => {
            let status = pb::TaskStatus::try_from(done.status)
                .map(task_status_label)
                .unwrap_or("unknown");
            format!("task:{} {status} {}", done.task_id, done.result_message)
        }
        pb::trigger::Kind::Heartbeat(_) => "heartbeat".to_string(),
        pb::trigger::Kind::Cron(cron) => format!("cron:{}", cron.key),
        pb::trigger::Kind::RefreshProfile(refresh) => {
            let scope = pb::RefreshScope::try_from(refresh.scope)
                .map(refresh_scope_label)
                .unwrap_or("unknown");
            format!("refresh:{scope}:{}", refresh.user_id)
        }
    }
}

fn emit_event(
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    session_id: &str,
    kind: pb::session_event::Kind,
) {
    let event = pb::SessionEvent {
        session_id: session_id.to_string(),
        created_at_unix_ms: now_unix_ms(),
        kind: Some(kind),
    };
    if events_tx.send(event).is_err() {
        warn!(%session_id, "dropping event because no subscribers are attached");
    }
}
