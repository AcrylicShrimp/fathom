use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::agent::ActionInvocation;
use crate::capability_domain::CapabilityDomainActorHandle;
use crate::runtime::Runtime;
use crate::session::diagnostics::execution_to_json;
use crate::session::state::SessionState;
use fathom_protocol::pb;

use super::events::emit_execution_update_event;
use super::tasks::{
    QueuedExecutionOutcome, queue_executions, queued_action_output, settled_execution_output,
};

pub(super) struct TurnActionDispatcher<'a> {
    runtime: &'a Runtime,
    state: &'a mut SessionState,
    events_tx: &'a broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &'a HashMap<String, CapabilityDomainActorHandle>,
    pending_action_invocations: Vec<ActionInvocation>,
    dispatched_actions: Vec<serde_json::Value>,
}

impl<'a> TurnActionDispatcher<'a> {
    pub(super) fn new(
        runtime: &'a Runtime,
        state: &'a mut SessionState,
        events_tx: &'a broadcast::Sender<pb::SessionEvent>,
        capability_domain_handles: &'a HashMap<String, CapabilityDomainActorHandle>,
    ) -> Self {
        Self {
            runtime,
            state,
            events_tx,
            capability_domain_handles,
            pending_action_invocations: Vec::new(),
            dispatched_actions: Vec::new(),
        }
    }

    pub(super) fn dispatch_action_invocation(&mut self, action_invocation: ActionInvocation) {
        self.pending_action_invocations.push(action_invocation);
    }

    pub(super) fn flush_action_invocations(&mut self) {
        if self.pending_action_invocations.is_empty() {
            return;
        }

        let queued_executions = queue_executions(
            self.runtime,
            self.state,
            self.events_tx,
            self.capability_domain_handles,
            std::mem::take(&mut self.pending_action_invocations),
        );
        for queued in queued_executions {
            let call_key = queued.call_key.clone();
            let call_id = queued.call_id.clone();
            let action_id = queued.execution.action_id.clone();
            let args_json = queued.execution.args_json.clone();
            let phase = match queued.outcome {
                QueuedExecutionOutcome::ForegroundAccepted => None,
                QueuedExecutionOutcome::BackgroundAccepted => {
                    Some(pb::ExecutionUpdatePhase::ExecutionBackgrounded)
                }
                QueuedExecutionOutcome::Rejected => {
                    Some(pb::ExecutionUpdatePhase::ExecutionRejected)
                }
            };
            let detail = match phase {
                Some(pb::ExecutionUpdatePhase::ExecutionBackgrounded) => {
                    queued_action_output(&queued.execution, call_id.as_deref(), true)
                }
                Some(pb::ExecutionUpdatePhase::ExecutionRejected) => settled_execution_output(
                    &queued.execution,
                    pb::ExecutionUpdatePhase::ExecutionRejected,
                ),
                _ => String::new(),
            };

            if let Some(phase) = phase {
                emit_execution_update_event(
                    self.events_tx,
                    &self.state.session_id,
                    phase,
                    call_key.clone(),
                    call_id.clone(),
                    Some(action_id.clone()),
                    Some(queued.execution.execution_id.clone()),
                    String::new(),
                    args_json.clone(),
                    detail,
                );
            }
            self.dispatched_actions.push(serde_json::json!({
                "action_id": action_id,
                "args_json": args_json,
                "call_key": call_key,
                "call_id": call_id,
                "queued_execution": execution_to_json(&queued.execution),
                "dispatch_outcome": match queued.outcome {
                    QueuedExecutionOutcome::ForegroundAccepted => "foreground_accepted",
                    QueuedExecutionOutcome::BackgroundAccepted => "background_accepted",
                    QueuedExecutionOutcome::Rejected => "rejected",
                },
            }));
        }
    }

    pub(super) fn action_dispatches(&self) -> &[serde_json::Value] {
        &self.dispatched_actions
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use tokio::sync::{broadcast, mpsc};

    use super::TurnActionDispatcher;
    use crate::agent::ActionInvocation;
    use crate::capability_domain::{
        build_default_capability_domain_registry, spawn_capability_domain_actor,
    };
    use crate::runtime::Runtime;
    use crate::session::{SessionCommand, SessionState};
    use crate::util::{default_agent_profile, default_user_profile};
    use fathom_capability_domain::CapabilityDomainSessionContext;
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
    fn dispatch_action_invocation_records_dispatch_and_emits_rejected_execution_update_without_runtime()
     {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let capability_domain_handles = HashMap::new();

        let mut dispatcher =
            TurnActionDispatcher::new(&runtime, &mut state, &events_tx, &capability_domain_handles);
        dispatcher.dispatch_action_invocation(ActionInvocation {
            action_id: "shell__run".to_string(),
            args_json: "{\"command\":\"pwd\",\"background\":true}".to_string(),
            call_key: "call-key-1".to_string(),
            call_id: Some("call-id-1".to_string()),
        });
        dispatcher.flush_action_invocations();

        assert_eq!(dispatcher.action_dispatches().len(), 1);

        let mut execution_update = None;
        while let Ok(event) = events_rx.try_recv() {
            if let Some(pb::session_event::Kind::ExecutionUpdate(item)) = event.kind {
                execution_update = Some(item);
                break;
            }
        }

        let execution_update = execution_update.expect("rejected execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::ExecutionRejected as i32
        );
        assert_eq!(execution_update.call_key, "call-key-1");
        assert_eq!(execution_update.call_id, "call-id-1");
        assert_eq!(execution_update.action_id, "shell__run");
        assert!(!execution_update.execution_id.is_empty());
        assert!(execution_update.detail.contains("execution_rejected"));
    }

    #[tokio::test]
    async fn dispatch_action_invocation_emits_execution_backgrounded_for_background_action() {
        let runtime = Runtime::new(2, 10);
        let (events_tx, mut events_rx) = broadcast::channel(16);
        let mut state = test_state();
        let (session_command_tx, _session_command_rx) = mpsc::channel::<SessionCommand>(16);
        let shell_instance = runtime
            .capability_domain_registry()
            .domain_factory("shell")
            .expect("shell factory")
            .create_instance(CapabilityDomainSessionContext {
                session_id: state.session_id.clone(),
            });
        let shell_handle =
            spawn_capability_domain_actor("shell".to_string(), shell_instance, session_command_tx);
        let capability_domain_handles = HashMap::from([("shell".to_string(), shell_handle)]);

        let mut dispatcher =
            TurnActionDispatcher::new(&runtime, &mut state, &events_tx, &capability_domain_handles);
        dispatcher.dispatch_action_invocation(ActionInvocation {
            action_id: "shell__run".to_string(),
            args_json: "{\"command\":\"pwd\",\"background\":true}".to_string(),
            call_key: "call-key-1".to_string(),
            call_id: Some("call-id-1".to_string()),
        });
        dispatcher.flush_action_invocations();

        let mut execution_update = None;
        while let Ok(event) = events_rx.try_recv() {
            if let Some(pb::session_event::Kind::ExecutionUpdate(item)) = event.kind {
                execution_update = Some(item);
                break;
            }
        }

        let execution_update = execution_update.expect("backgrounded execution update event");
        assert_eq!(
            execution_update.phase,
            pb::ExecutionUpdatePhase::ExecutionBackgrounded as i32
        );
        assert_eq!(execution_update.call_key, "call-key-1");
        assert_eq!(execution_update.call_id, "call-id-1");
        assert_eq!(execution_update.action_id, "shell__run");
        assert!(!execution_update.execution_id.is_empty());
        assert!(execution_update.detail.contains("background=true"));
    }
}
