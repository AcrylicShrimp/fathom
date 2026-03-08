use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::agent::{ModelDeltaEvent, StreamNote};
use crate::environment::EnvironmentActorHandle;
use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;

use super::assistant_stream::TurnAssistantStreamEmitter;
use super::events::emit_event;
use super::tool_dispatch::{TurnToolDispatcher, emit_tool_call_event};

pub(super) struct TurnDeltaTransport<'a> {
    session_id: String,
    events_tx: &'a broadcast::Sender<pb::SessionEvent>,
    stream_emitter: TurnAssistantStreamEmitter,
    invocation_stream_notes: Vec<serde_json::Value>,
    streamed_assistant_outputs: Vec<(String, String)>,
    tool_dispatcher: TurnToolDispatcher<'a>,
}

impl<'a> TurnDeltaTransport<'a> {
    pub(super) fn new(
        runtime: &'a Runtime,
        state: &'a mut SessionState,
        events_tx: &'a broadcast::Sender<pb::SessionEvent>,
        environment_handles: &'a HashMap<String, EnvironmentActorHandle>,
        turn_id: u64,
    ) -> Self {
        let session_id = state.session_id.clone();
        Self {
            session_id,
            events_tx,
            stream_emitter: TurnAssistantStreamEmitter::new(turn_id),
            invocation_stream_notes: Vec::new(),
            streamed_assistant_outputs: Vec::new(),
            tool_dispatcher: TurnToolDispatcher::new(
                runtime,
                state,
                events_tx,
                environment_handles,
            ),
        }
    }

    pub(super) fn handle_model_event(&mut self, event: ModelDeltaEvent) {
        match event {
            ModelDeltaEvent::StreamNote(note) => self.on_stream_note(note),
            ModelDeltaEvent::ActionInvocation(action_invocation) => {
                self.tool_dispatcher
                    .dispatch_action_invocation(action_invocation);
            }
            ModelDeltaEvent::ActionArgsDelta(note) => emit_tool_call_event(
                self.events_tx,
                &self.session_id,
                pb::ToolCallPhase::ArgumentsDelta,
                note.call_key,
                note.call_id,
                note.action_id,
                None,
                note.args_delta,
                String::new(),
                String::new(),
            ),
            ModelDeltaEvent::ActionArgsDone(note) => emit_tool_call_event(
                self.events_tx,
                &self.session_id,
                pb::ToolCallPhase::ArgumentsReady,
                note.call_key,
                note.call_id,
                note.action_id,
                None,
                String::new(),
                note.args_json,
                String::new(),
            ),
            ModelDeltaEvent::AssistantTextDelta(delta) => {
                let session_id = self.session_id.clone();
                let events_tx = self.events_tx;
                self.stream_emitter.on_assistant_text_delta(&delta, |kind| {
                    emit_event(events_tx, &session_id, kind)
                });
            }
            ModelDeltaEvent::AssistantTextDone(text) => {
                let session_id = self.session_id.clone();
                let events_tx = self.events_tx;
                let stream_id = self.stream_emitter.stream_id();
                let content = self
                    .stream_emitter
                    .on_assistant_text_done(Some(&text), |kind| {
                        emit_event(events_tx, &session_id, kind)
                    });
                self.streamed_assistant_outputs.push((stream_id, content));
            }
        }
    }

    pub(super) fn invocation_stream_notes(&self) -> &[serde_json::Value] {
        &self.invocation_stream_notes
    }

    pub(super) fn action_dispatches(&self) -> &[serde_json::Value] {
        self.tool_dispatcher.action_dispatches()
    }

    pub(super) fn drain_streamed_assistant_outputs(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.streamed_assistant_outputs)
    }

    fn on_stream_note(&mut self, note: StreamNote) {
        if note.phase != "openai.stream.event" {
            self.invocation_stream_notes.push(serde_json::json!({
                "phase": note.phase.clone(),
                "detail": note.detail.clone(),
            }));
        }
        emit_event(
            self.events_tx,
            &self.session_id,
            pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                phase: note.phase,
                detail: note.detail,
                created_at_unix_ms: now_unix_ms(),
            }),
        );
    }
}
