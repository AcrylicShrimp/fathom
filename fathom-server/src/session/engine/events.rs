use tokio::sync::broadcast;
use tracing::warn;

use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;

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
