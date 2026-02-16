use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::agent::{
    ActionArgDeltaNote, ActionArgDoneNote, ActionInvocation, StreamNote, render_prompt,
};
use crate::environment::EnvironmentActorHandle;
use crate::pb;
use crate::runtime::Runtime;
use crate::session::diagnostics::{task_to_json, trigger_to_json, turn_snapshot_to_json};
use crate::session::state::{SessionCommand, SessionState};
use crate::util::now_unix_ms;

use super::assistant_stream::TurnAssistantStreamEmitter;
use super::events::emit_event;
use super::history_flush::flush_history;
use super::profiles::apply_profile_refresh;
use super::tasks::{queue_task, queued_action_output};

#[derive(Debug, Clone, Copy)]
struct AgentTurnSummary {
    action_call_count: usize,
    assistant_output_count: usize,
}

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
        runtime.diagnostics().append_session_record(
            &state.session_id,
            serde_json::json!({
                "ts_unix_ms": now_unix_ms(),
                "event": "turn.started",
                "session_id": state.session_id,
                "turn_id": turn_id,
                "trigger_count": turn_triggers.len(),
                "triggers": turn_triggers.iter().map(trigger_to_json).collect::<Vec<_>>(),
            }),
        );

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
        let mut agent_summary = None;

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
            let invocation_seq = state.allocate_agent_invocation_seq();
            agent_summary = Some(
                run_agent_turn(
                    runtime,
                    state,
                    events_tx,
                    environment_handles,
                    turn_id,
                    invocation_seq,
                    &agent_triggers,
                    &mut assistant_outputs,
                    &mut assistant_stream_ids,
                )
                .await,
            );
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

        let is_quiescent = agent_summary.is_some_and(|summary| {
            summary.assistant_output_count > 0
                && summary.action_call_count == 0
                && state.in_flight_actions.is_empty()
                && state.trigger_queue.is_empty()
        });
        if is_quiescent {
            state.pending_payload_lookups.clear();
        }
        runtime.diagnostics().append_session_record(
            &state.session_id,
            serde_json::json!({
                "ts_unix_ms": now_unix_ms(),
                "event": "turn.ended",
                "session_id": state.session_id,
                "turn_id": turn_id,
                "history_size": state.history.len(),
                "pending_trigger_count": state.trigger_queue.len(),
                "in_flight_action_count": state.in_flight_actions.len(),
                "agent_summary": agent_summary.map(|summary| serde_json::json!({
                    "action_call_count": summary.action_call_count,
                    "assistant_output_count": summary.assistant_output_count,
                })),
                "quiescent": is_quiescent,
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
    invocation_seq: u64,
    agent_triggers: &[pb::Trigger],
    assistant_outputs: &mut Vec<String>,
    assistant_stream_ids: &mut Vec<String>,
) -> AgentTurnSummary {
    let assistant_output_start_len = assistant_outputs.len();
    let snapshot = runtime.build_turn_snapshot(state, turn_id, agent_triggers);
    let prompt = render_prompt(&snapshot, None);
    let invocation_detail_path = format!(
        "sessions/{}/invocations/invocation-{}.json",
        state.session_id, invocation_seq
    );
    runtime.diagnostics().write_invocation_context(
        &state.session_id,
        invocation_seq,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "agent.invocation.context",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "invocation_seq": invocation_seq,
            "snapshot": turn_snapshot_to_json(&snapshot),
            "prompt": prompt,
        }),
    );
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "agent.invocation.started",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "invocation_seq": invocation_seq,
            "context_path": invocation_detail_path,
        }),
    );
    let orchestrator = runtime.agent_orchestrator();
    let session_id = state.session_id.clone();
    let stream_emitter = Arc::new(Mutex::new(TurnAssistantStreamEmitter::new(turn_id)));
    let streamed_assistant_outputs = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let invocation_stream_notes = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let dispatched_actions = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));

    let outcome = orchestrator
        .run_turn(
            &snapshot,
            {
                let session_id = session_id.clone();
                let invocation_stream_notes = invocation_stream_notes.clone();
                move |note: StreamNote| {
                    if note.phase != "openai.stream.event" {
                        invocation_stream_notes
                            .lock()
                            .expect("invocation stream notes lock poisoned")
                            .push(serde_json::json!({
                                "phase": note.phase.clone(),
                                "detail": note.detail.clone(),
                            }));
                    }
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
                let action_id = action_invocation.action_id;
                let args_json = action_invocation.args_json;
                let call_key = action_invocation.call_key;
                let call_id = action_invocation.call_id;
                let task = queue_task(
                    runtime,
                    state,
                    events_tx,
                    environment_handles,
                    action_id.clone(),
                    args_json.clone(),
                );

                emit_event(
                    events_tx,
                    &session_id,
                    pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                        phase: "action.queued".to_string(),
                        detail: queued_action_output(&task, call_id.as_deref()),
                        created_at_unix_ms: now_unix_ms(),
                    }),
                );
                dispatched_actions
                    .lock()
                    .expect("dispatched actions lock poisoned")
                    .push(serde_json::json!({
                        "action_id": action_id,
                        "args_json": args_json,
                        "call_key": call_key,
                        "call_id": call_id,
                        "queued_task": task_to_json(&task),
                    }));
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
    let action_call_count = outcome.action_call_count;
    let mut model_assistant_outputs = outcome.assistant_outputs;
    let model_diagnostics = outcome.diagnostics;
    let failed = outcome.failed;
    let failure_code = outcome.failure_code;
    let failure_message = outcome.failure_message;

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

    for output in model_assistant_outputs.drain(..) {
        if output.trim().is_empty() {
            continue;
        }
        if assistant_outputs.last().is_some_and(|last| last == &output) {
            continue;
        }
        assistant_outputs.push(output);
        assistant_stream_ids.push(String::new());
    }

    for diagnostic in &model_diagnostics {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                phase: "agent.diagnostic".to_string(),
                detail: diagnostic.clone(),
                created_at_unix_ms: now_unix_ms(),
            }),
        );
    }

    if failed {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TurnFailure(pb::TurnFailureEvent {
                turn_id,
                reason_code: failure_code.clone(),
                message: failure_message.clone(),
            }),
        );
        assistant_outputs.push(format!(
            "turn failed [{}]: {}",
            failure_code, failure_message
        ));
        assistant_stream_ids.push(String::new());
    }

    let assistant_outputs_slice = assistant_outputs
        .iter()
        .skip(assistant_output_start_len)
        .cloned()
        .collect::<Vec<_>>();
    let stream_notes = invocation_stream_notes
        .lock()
        .expect("invocation stream notes lock poisoned")
        .clone();
    let action_dispatches = dispatched_actions
        .lock()
        .expect("dispatched actions lock poisoned")
        .clone();
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "agent.invocation.finished",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "invocation_seq": invocation_seq,
            "failed": failed,
            "failure_code": failure_code,
            "failure_message": failure_message,
            "action_call_count": action_call_count,
            "assistant_outputs": assistant_outputs_slice,
            "diagnostics": model_diagnostics,
            "stream_notes": stream_notes,
            "action_dispatches": action_dispatches,
        }),
    );

    AgentTurnSummary {
        action_call_count,
        assistant_output_count: assistant_outputs
            .len()
            .saturating_sub(assistant_output_start_len),
    }
}
