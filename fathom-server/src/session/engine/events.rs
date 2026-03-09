use tokio::sync::broadcast;
use tracing::warn;

use crate::runtime::Runtime;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;
use fathom_protocol::pb;

pub(super) fn enqueue_automatic_heartbeat(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
) {
    let trigger = pb::Trigger {
        trigger_id: runtime.next_trigger_id(),
        created_at_unix_ms: now_unix_ms(),
        kind: Some(pb::trigger::Kind::Heartbeat(pb::HeartbeatTrigger {})),
    };
    enqueue_trigger(state, events_tx, trigger);
}

pub(super) fn enqueue_trigger(
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

pub(super) fn emit_event(
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

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_execution_update_event(
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    session_id: &str,
    phase: pb::ExecutionUpdatePhase,
    call_key: String,
    call_id: Option<String>,
    action_id: Option<String>,
    execution_id: Option<String>,
    args_delta: String,
    args_json: String,
    detail: String,
) {
    emit_event(
        events_tx,
        session_id,
        pb::session_event::Kind::ExecutionUpdate(pb::ExecutionUpdateEvent {
            phase: phase as i32,
            call_key,
            call_id: call_id.unwrap_or_default(),
            action_id: action_id.unwrap_or_default(),
            execution_id: execution_id.unwrap_or_default(),
            args_delta,
            args_json,
            detail,
        }),
    );
}
