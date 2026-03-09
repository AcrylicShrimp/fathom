use serde_json::{Value, json};

use crate::agent::AgentInvocationContext;
use fathom_protocol::execution_status_label;
use fathom_protocol::pb;

pub(crate) fn execution_to_json(execution: &pb::Execution) -> Value {
    let status = pb::ExecutionStatus::try_from(execution.status)
        .map(execution_status_label)
        .unwrap_or("unknown");

    json!({
        "execution_id": execution.execution_id,
        "session_id": execution.session_id,
        "action_id": execution.action_id,
        "args_json": execution.args_json,
        "status": status,
        "result_message": execution.result_message,
        "created_at_unix_ms": execution.created_at_unix_ms,
        "updated_at_unix_ms": execution.updated_at_unix_ms,
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
        Some(pb::trigger::Kind::ExecutionUpdate(update)) => json!({
            "type": "execution_update",
            "execution_id": update.execution_id,
            "action_id": update.action_id,
            "kind": pb::ExecutionUpdateKind::try_from(update.kind)
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|_| "Unspecified".to_string()),
            "message": update.message,
            "payload_message": update.payload_message,
        }),
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

pub(crate) fn agent_invocation_context_to_json(context: &AgentInvocationContext) -> Value {
    let triggers = context
        .triggers
        .iter()
        .map(trigger_to_json)
        .collect::<Vec<_>>();

    json!({
        "harness_contract": serde_json::to_value(&context.harness_contract)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_harness_contract"})),
        "identity_envelope": serde_json::to_value(&context.identity_envelope)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_identity_envelope"})),
        "session_baseline": serde_json::to_value(&context.session_baseline)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_session_baseline"})),
        "resolved_payload_lookups": context.resolved_payload_lookups,
        "triggers": triggers,
        "recent_history": context.recent_history,
        "compaction": serde_json::to_value(&context.compaction)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_compaction"})),
    })
}
