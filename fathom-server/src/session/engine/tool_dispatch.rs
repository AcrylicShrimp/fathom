use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::agent::ActionInvocation;
use crate::environment::EnvironmentActorHandle;
use crate::pb;
use crate::runtime::Runtime;
use crate::session::diagnostics::task_to_json;
use crate::session::state::SessionState;

use super::events::emit_event;
use super::tasks::{queue_task, queued_action_output};

pub(super) struct TurnToolDispatcher<'a> {
    runtime: &'a Runtime,
    state: &'a mut SessionState,
    events_tx: &'a broadcast::Sender<pb::SessionEvent>,
    environment_handles: &'a HashMap<String, EnvironmentActorHandle>,
    dispatched_actions: Vec<serde_json::Value>,
}

impl<'a> TurnToolDispatcher<'a> {
    pub(super) fn new(
        runtime: &'a Runtime,
        state: &'a mut SessionState,
        events_tx: &'a broadcast::Sender<pb::SessionEvent>,
        environment_handles: &'a HashMap<String, EnvironmentActorHandle>,
    ) -> Self {
        Self {
            runtime,
            state,
            events_tx,
            environment_handles,
            dispatched_actions: Vec::new(),
        }
    }

    pub(super) fn dispatch_action_invocation(&mut self, action_invocation: ActionInvocation) {
        let action_id = action_invocation.action_id;
        let args_json = action_invocation.args_json;
        let call_key = action_invocation.call_key;
        let call_id = action_invocation.call_id;
        let task = queue_task(
            self.runtime,
            self.state,
            self.events_tx,
            self.environment_handles,
            action_id.clone(),
            args_json.clone(),
        );

        emit_tool_call_event(
            self.events_tx,
            &self.state.session_id,
            pb::ToolCallPhase::Queued,
            call_key.clone(),
            call_id.clone(),
            Some(action_id.clone()),
            Some(task.task_id.clone()),
            String::new(),
            args_json.clone(),
            queued_action_output(&task, call_id.as_deref()),
        );
        self.dispatched_actions.push(serde_json::json!({
            "action_id": action_id,
            "args_json": args_json,
            "call_key": call_key,
            "call_id": call_id,
            "queued_task": task_to_json(&task),
        }));
    }

    pub(super) fn action_dispatches(&self) -> &[serde_json::Value] {
        &self.dispatched_actions
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_tool_call_event(
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    session_id: &str,
    phase: pb::ToolCallPhase,
    call_key: String,
    call_id: Option<String>,
    action_id: Option<String>,
    task_id: Option<String>,
    args_delta: String,
    args_json: String,
    detail: String,
) {
    emit_event(
        events_tx,
        session_id,
        pb::session_event::Kind::ToolCall(pb::ToolCallEvent {
            phase: phase as i32,
            call_key,
            call_id: call_id.unwrap_or_default(),
            action_id: action_id.unwrap_or_default(),
            task_id: task_id.unwrap_or_default(),
            args_delta,
            args_json,
            detail,
        }),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use tokio::sync::broadcast;

    use super::TurnToolDispatcher;
    use crate::agent::ActionInvocation;
    use crate::environment::EnvironmentRegistry;
    use crate::pb;
    use crate::runtime::Runtime;
    use crate::session::SessionState;
    use crate::util::{default_agent_profile, default_user_profile};

    fn test_state() -> SessionState {
        let user_id = "user-a".to_string();
        SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            EnvironmentRegistry::default_engaged_environment_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            EnvironmentRegistry::initial_environment_snapshots()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        )
    }

    #[test]
    fn dispatch_action_invocation_records_dispatch_and_emits_queued_tool_call() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let environment_handles = HashMap::new();

        let mut dispatcher =
            TurnToolDispatcher::new(&runtime, &mut state, &events_tx, &environment_handles);
        dispatcher.dispatch_action_invocation(ActionInvocation {
            action_id: "filesystem__list".to_string(),
            args_json: "{\"path\":\".\"}".to_string(),
            call_key: "call-key-1".to_string(),
            call_id: Some("call-id-1".to_string()),
        });

        assert_eq!(dispatcher.action_dispatches().len(), 1);

        let mut tool_call = None;
        while let Ok(event) = events_rx.try_recv() {
            if let Some(pb::session_event::Kind::ToolCall(item)) = event.kind {
                tool_call = Some(item);
                break;
            }
        }

        let tool_call = tool_call.expect("queued tool call event");
        assert_eq!(tool_call.phase, pb::ToolCallPhase::Queued as i32);
        assert_eq!(tool_call.call_key, "call-key-1");
        assert_eq!(tool_call.call_id, "call-id-1");
        assert_eq!(tool_call.action_id, "filesystem__list");
        assert!(!tool_call.task_id.is_empty());
        assert!(
            tool_call
                .detail
                .contains("queued action `filesystem__list`")
        );
    }
}
