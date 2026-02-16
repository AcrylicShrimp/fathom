use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::agent::{ActionArgDeltaNote, ActionArgDoneNote, ActionInvocation, StreamNote};
use crate::environment::EnvironmentActorHandle;
use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::{SessionCommand, SessionState};
use crate::util::now_unix_ms;

use super::assistant_stream::TurnAssistantStreamEmitter;
use super::events::emit_event;
use super::history_flush::flush_history;
use super::profiles::apply_profile_refresh;
use super::tasks::{queue_task, queued_action_output};

pub(super) async fn process_turns(
    runtime: &Runtime,
    state: &mut SessionState,
    _command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    environment_handles: &HashMap<String, EnvironmentActorHandle>,
) {
    if state.turn_in_progress
        || state.trigger_queue.is_empty()
        || !state.in_flight_actions.is_empty()
    {
        return;
    }

    state.turn_in_progress = true;
    while !state.trigger_queue.is_empty() && state.in_flight_actions.is_empty() {
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
        let mut assistant_stream_ids = Vec::new();
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
                    assistant_stream_ids.push(String::new());
                }
                _ => agent_triggers.push(trigger.clone()),
            }
        }

        if !agent_triggers.is_empty() {
            run_agent_turn(
                runtime,
                state,
                events_tx,
                environment_handles,
                turn_id,
                &agent_triggers,
                &mut assistant_outputs,
                &mut assistant_stream_ids,
            )
            .await;
        }

        for (index, output) in assistant_outputs.iter().enumerate() {
            let stream_id = assistant_stream_ids.get(index).cloned().unwrap_or_default();
            emit_event(
                events_tx,
                &state.session_id,
                pb::session_event::Kind::AssistantOutput(pb::AssistantOutputEvent {
                    content: output.clone(),
                    stream_id,
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
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    environment_handles: &HashMap<String, EnvironmentActorHandle>,
    turn_id: u64,
    agent_triggers: &[pb::Trigger],
    assistant_outputs: &mut Vec<String>,
    assistant_stream_ids: &mut Vec<String>,
) {
    let snapshot = runtime.build_turn_snapshot(state, turn_id, agent_triggers);
    let orchestrator = runtime.agent_orchestrator();
    let session_id = state.session_id.clone();
    let stream_emitter = Arc::new(Mutex::new(TurnAssistantStreamEmitter::new(turn_id)));
    let streamed_assistant_outputs = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

    let outcome = orchestrator
        .run_turn(
            &snapshot,
            {
                let session_id = session_id.clone();
                move |note: StreamNote| {
                    emit_event(
                        events_tx,
                        &session_id,
                        pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                            phase: note.phase,
                            detail: note.detail,
                            created_at_unix_ms: now_unix_ms(),
                        }),
                    );
                }
            },
            |action_invocation: ActionInvocation| {
                let task = queue_task(
                    runtime,
                    state,
                    events_tx,
                    environment_handles,
                    action_invocation.action_id,
                    action_invocation.args_json,
                );

                emit_event(
                    events_tx,
                    &session_id,
                    pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                        phase: "action.queued".to_string(),
                        detail: queued_action_output(&task, action_invocation.call_id.as_deref()),
                        created_at_unix_ms: now_unix_ms(),
                    }),
                );
            },
            {
                let session_id = session_id.clone();
                let stream_emitter = stream_emitter.clone();
                move |note: ActionArgDeltaNote| {
                    stream_emitter
                        .lock()
                        .expect("stream emitter lock poisoned")
                        .on_action_args_delta(&note, |kind| {
                            emit_event(events_tx, &session_id, kind)
                        });
                }
            },
            {
                let session_id = session_id.clone();
                let stream_emitter = stream_emitter.clone();
                move |note: ActionArgDoneNote| {
                    stream_emitter
                        .lock()
                        .expect("stream emitter lock poisoned")
                        .on_action_args_done(&note, |kind| {
                            emit_event(events_tx, &session_id, kind)
                        });
                }
            },
            {
                let session_id = session_id.clone();
                let stream_emitter = stream_emitter.clone();
                move |delta: String| {
                    stream_emitter
                        .lock()
                        .expect("stream emitter lock poisoned")
                        .on_assistant_text_delta(&delta, |kind| {
                            emit_event(events_tx, &session_id, kind)
                        });
                }
            },
            {
                let session_id = session_id.clone();
                let stream_emitter = stream_emitter.clone();
                let streamed_assistant_outputs = streamed_assistant_outputs.clone();
                move |text: String| {
                    let stream_id = stream_emitter
                        .lock()
                        .expect("stream emitter lock poisoned")
                        .stream_id();
                    let content = stream_emitter
                        .lock()
                        .expect("stream emitter lock poisoned")
                        .on_assistant_text_done(Some(&text), |kind| {
                            emit_event(events_tx, &session_id, kind)
                        });
                    streamed_assistant_outputs
                        .lock()
                        .expect("streamed assistant outputs lock poisoned")
                        .push((stream_id, content));
                }
            },
        )
        .await;

    let streamed_outputs = {
        let mut lock = streamed_assistant_outputs
            .lock()
            .expect("streamed assistant outputs lock poisoned");
        std::mem::take(&mut *lock)
    };
    for (stream_id, output) in streamed_outputs {
        assistant_outputs.push(output);
        assistant_stream_ids.push(stream_id);
    }

    for output in outcome.assistant_outputs {
        if output.trim().is_empty() {
            continue;
        }
        if assistant_outputs.last().is_some_and(|last| last == &output) {
            continue;
        }
        assistant_outputs.push(output);
        assistant_stream_ids.push(String::new());
    }

    for diagnostic in outcome.diagnostics {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                phase: "agent.diagnostic".to_string(),
                detail: diagnostic,
                created_at_unix_ms: now_unix_ms(),
            }),
        );
    }

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
        assistant_stream_ids.push(String::new());
    }
}
