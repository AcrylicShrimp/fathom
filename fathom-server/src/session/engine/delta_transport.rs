use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::agent::{ModelDeltaEvent, StreamNote};
use crate::capability_domain::CapabilityDomainActorHandle;
use crate::runtime::Runtime;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;
use fathom_protocol::pb;

use super::action_dispatch::TurnActionDispatcher;
use super::assistant_stream::TurnAssistantStreamEmitter;
use super::events::{emit_event, emit_execution_update_event};

pub(super) struct TurnDeltaTransport<'a> {
    session_id: String,
    events_tx: &'a broadcast::Sender<pb::SessionEvent>,
    stream_emitter: TurnAssistantStreamEmitter,
    invocation_stream_notes: Vec<serde_json::Value>,
    streamed_assistant_outputs: Vec<(String, String)>,
    action_dispatcher: TurnActionDispatcher<'a>,
}

impl<'a> TurnDeltaTransport<'a> {
    pub(super) fn new(
        runtime: &'a Runtime,
        state: &'a mut SessionState,
        events_tx: &'a broadcast::Sender<pb::SessionEvent>,
        capability_domain_handles: &'a HashMap<String, CapabilityDomainActorHandle>,
        turn_id: u64,
    ) -> Self {
        let session_id = state.session_id.clone();
        Self {
            session_id,
            events_tx,
            stream_emitter: TurnAssistantStreamEmitter::new(turn_id),
            invocation_stream_notes: Vec::new(),
            streamed_assistant_outputs: Vec::new(),
            action_dispatcher: TurnActionDispatcher::new(
                runtime,
                state,
                events_tx,
                capability_domain_handles,
            ),
        }
    }

    pub(super) fn handle_model_event(&mut self, event: ModelDeltaEvent) {
        match event {
            ModelDeltaEvent::StreamNote(note) => self.on_stream_note(note),
            ModelDeltaEvent::ActionInvocation(action_invocation) => {
                self.action_dispatcher
                    .dispatch_action_invocation(action_invocation);
            }
            ModelDeltaEvent::ActionArgsDelta(note) => emit_execution_update_event(
                self.events_tx,
                &self.session_id,
                pb::ExecutionUpdatePhase::ArgumentsDelta,
                note.call_key,
                note.call_id,
                note.action_id,
                None,
                note.args_delta,
                String::new(),
                String::new(),
            ),
            ModelDeltaEvent::ActionArgsDone(note) => emit_execution_update_event(
                self.events_tx,
                &self.session_id,
                pb::ExecutionUpdatePhase::ArgumentsReady,
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
        self.action_dispatcher.action_dispatches()
    }

    pub(super) fn flush_action_invocations(&mut self) {
        self.action_dispatcher.flush_action_invocations();
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use tokio::sync::broadcast;

    use super::TurnDeltaTransport;
    use crate::agent::{ActionArgDeltaNote, ActionArgDoneNote, ModelDeltaEvent, StreamNote};
    use crate::capability_domain::CapabilityDomainActorHandle;
    use crate::capability_domain::build_default_capability_domain_registry;
    use crate::runtime::Runtime;
    use crate::session::SessionState;
    use crate::util::{default_agent_profile, default_user_profile};
    use fathom_protocol::pb;

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

    #[test]
    fn delta_transport_preserves_event_order_for_stream_notes_argument_updates_and_text_streams() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(32);
        let mut state = test_state();
        let capability_domain_handles = HashMap::<String, CapabilityDomainActorHandle>::new();
        let mut transport = TurnDeltaTransport::new(
            &runtime,
            &mut state,
            &events_tx,
            &capability_domain_handles,
            7,
        );

        transport.handle_model_event(ModelDeltaEvent::StreamNote(StreamNote {
            phase: "agent.test".to_string(),
            detail: "begin".to_string(),
        }));
        transport.handle_model_event(ModelDeltaEvent::ActionArgsDelta(ActionArgDeltaNote {
            call_key: "call-key-1".to_string(),
            call_id: Some("call-id-1".to_string()),
            action_id: Some("filesystem__read".to_string()),
            args_delta: "{\"path\":\"".to_string(),
        }));
        transport.handle_model_event(ModelDeltaEvent::ActionArgsDone(ActionArgDoneNote {
            call_key: "call-key-1".to_string(),
            call_id: Some("call-id-1".to_string()),
            action_id: Some("filesystem__read".to_string()),
            args_json: "{\"path\":\"Cargo.toml\"}".to_string(),
        }));
        transport.handle_model_event(ModelDeltaEvent::AssistantTextDelta("hel".to_string()));
        transport.handle_model_event(ModelDeltaEvent::AssistantTextDone("hello".to_string()));

        let mut events = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            events.push(event);
        }

        assert!(matches!(
            events.first().and_then(|event| event.kind.as_ref()),
            Some(pb::session_event::Kind::AgentStream(pb::AgentStreamEvent { phase, detail, .. }))
                if phase == "agent.test" && detail == "begin"
        ));
        assert!(matches!(
            events.get(1).and_then(|event| event.kind.as_ref()),
            Some(pb::session_event::Kind::ExecutionUpdate(pb::ExecutionUpdateEvent { phase, call_key, action_id, args_delta, .. }))
                if *phase == pb::ExecutionUpdatePhase::ArgumentsDelta as i32
                    && call_key == "call-key-1"
                    && action_id == "filesystem__read"
                    && args_delta == "{\"path\":\""
        ));
        assert!(matches!(
            events.get(2).and_then(|event| event.kind.as_ref()),
            Some(pb::session_event::Kind::ExecutionUpdate(pb::ExecutionUpdateEvent { phase, call_key, action_id, args_json, .. }))
                if *phase == pb::ExecutionUpdatePhase::ArgumentsReady as i32
                    && call_key == "call-key-1"
                    && action_id == "filesystem__read"
                    && args_json.contains("\"Cargo.toml\"")
        ));

        let assistant_streams = events
            .iter()
            .filter_map(|event| match event.kind.as_ref() {
                Some(pb::session_event::Kind::AssistantStream(item)) => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(assistant_streams.len(), 3);
        assert_eq!(assistant_streams[0].stream_id, "7:assistant");
        assert_eq!(assistant_streams[0].delta, "hel");
        assert!(!assistant_streams[0].done);
        assert_eq!(assistant_streams[1].stream_id, "7:assistant");
        assert_eq!(assistant_streams[1].delta, "lo");
        assert!(!assistant_streams[1].done);
        assert_eq!(assistant_streams[2].stream_id, "7:assistant");
        assert!(assistant_streams[2].delta.is_empty());
        assert!(assistant_streams[2].done);

        let outputs = transport.drain_streamed_assistant_outputs();
        assert_eq!(
            outputs,
            vec![("7:assistant".to_string(), "hello".to_string())]
        );
    }
}
