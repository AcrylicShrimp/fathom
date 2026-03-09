use serde_json::{Value, json};

use crate::agent::TurnSnapshot;
use crate::pb;
use crate::util::task_status_label;

pub(crate) fn task_to_json(task: &pb::Task) -> Value {
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");

    json!({
        "task_id": task.task_id,
        "session_id": task.session_id,
        "action_id": task.action_id,
        "args_json": task.args_json,
        "status": status,
        "result_message": task.result_message,
        "created_at_unix_ms": task.created_at_unix_ms,
        "updated_at_unix_ms": task.updated_at_unix_ms,
    })
}

pub(crate) fn trigger_to_json(trigger: &pb::Trigger) -> Value {
    let kind = trigger.kind.as_ref();
    let kind_json = match kind {
        Some(pb::trigger::Kind::UserMessage(message)) => json!({
            "type": "user_message",
            "user_id": message.user_id,
            "text": message.text,
        }),
        Some(pb::trigger::Kind::TaskDone(done)) => {
            let status = pb::TaskStatus::try_from(done.status)
                .map(task_status_label)
                .unwrap_or("unknown");
            json!({
                "type": "task_done",
                "task_id": done.task_id,
                "status": status,
                "result_message": done.result_message,
            })
        }
        Some(pb::trigger::Kind::Heartbeat(_)) => json!({ "type": "heartbeat" }),
        Some(pb::trigger::Kind::Cron(cron)) => json!({
            "type": "cron",
            "key": cron.key,
        }),
        Some(pb::trigger::Kind::RefreshProfile(refresh)) => json!({
            "type": "refresh_profile",
            "scope": refresh.scope,
            "user_id": refresh.user_id,
        }),
        None => json!({ "type": "unknown" }),
    };

    json!({
        "trigger_id": trigger.trigger_id,
        "created_at_unix_ms": trigger.created_at_unix_ms,
        "kind": kind_json,
    })
}

pub(crate) fn turn_snapshot_to_json(snapshot: &TurnSnapshot) -> Value {
    let triggers = snapshot
        .triggers
        .iter()
        .map(trigger_to_json)
        .collect::<Vec<_>>();

    json!({
        "session_id": snapshot.session_id,
        "turn_id": snapshot.turn_id,
        "harness_contract": serde_json::to_value(&snapshot.harness_contract)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_harness_contract"})),
        "identity_envelope": serde_json::to_value(&snapshot.identity_envelope)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_identity_envelope"})),
        "session_baseline": serde_json::to_value(&snapshot.session_baseline)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_session_baseline"})),
        "in_flight_actions": serde_json::to_value(&snapshot.in_flight_actions)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_in_flight_actions"})),
        "resolved_payload_lookups": snapshot.resolved_payload_lookups,
        "triggers": triggers,
        "recent_history": snapshot.recent_history,
        "compaction": serde_json::to_value(&snapshot.compaction)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_compaction"})),
    })
}
