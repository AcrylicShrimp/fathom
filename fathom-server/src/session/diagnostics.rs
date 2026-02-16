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
    let participant_profiles = snapshot
        .participant_profiles
        .iter()
        .map(user_profile_to_json)
        .collect::<Vec<_>>();
    let triggers = snapshot
        .triggers
        .iter()
        .map(trigger_to_json)
        .collect::<Vec<_>>();

    json!({
        "session_id": snapshot.session_id,
        "turn_id": snapshot.turn_id,
        "system_context": serde_json::to_value(&snapshot.system_context)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_system_context"})),
        "agent_profile": agent_profile_to_json(&snapshot.agent_profile),
        "participant_profiles": participant_profiles,
        "resolved_payload_lookups": snapshot.resolved_payload_lookups,
        "triggers": triggers,
        "recent_history": snapshot.recent_history,
        "compaction": serde_json::to_value(&snapshot.compaction)
            .unwrap_or_else(|_| json!({"error": "failed_to_serialize_compaction"})),
    })
}

pub(crate) fn user_profile_to_json(profile: &pb::UserProfile) -> Value {
    json!({
        "user_id": profile.user_id,
        "name": profile.name,
        "nickname": profile.nickname,
        "preferences_json": profile.preferences_json,
        "user_md": profile.user_md,
        "long_term_memory_md": profile.long_term_memory_md,
        "updated_at_unix_ms": profile.updated_at_unix_ms,
    })
}

fn agent_profile_to_json(profile: &pb::AgentProfile) -> Value {
    json!({
        "agent_id": profile.agent_id,
        "display_name": profile.display_name,
        "soul_md": profile.soul_md,
        "identity_md": profile.identity_md,
        "agents_md": profile.agents_md,
        "guidelines_md": profile.guidelines_md,
        "code_of_conduct_md": profile.code_of_conduct_md,
        "long_term_memory_md": profile.long_term_memory_md,
        "spec_version": profile.spec_version,
        "updated_at_unix_ms": profile.updated_at_unix_ms,
    })
}
