use fathom_env::parse_action_id;
use serde_json::json;

use crate::history::preview::build_payload_preview;
use crate::history::schema::{HistoryActorKind, HistoryEventLine};
use crate::history::{TASK_FINISHED_EVENT, TASK_PAYLOAD_LOOKUP_ACTION, TASK_STARTED_EVENT};
use crate::pb;
use crate::session::state::SessionState;
use crate::util::{refresh_scope_label, task_status_label};

pub(crate) fn trigger_line(state: &SessionState, trigger: &pb::Trigger) -> String {
    let Some(kind) = trigger.kind.as_ref() else {
        return HistoryEventLine {
            ts_unix_ms: trigger.created_at_unix_ms,
            event: "trigger_unknown".to_string(),
            actor_kind: HistoryActorKind::System,
            actor_id: "runtime".to_string(),
            profile_ref: active_agent_profile_ref(state),
            payload: json!({}),
        }
        .to_json_line();
    };

    match kind {
        pb::trigger::Kind::UserMessage(message) => HistoryEventLine {
            ts_unix_ms: trigger.created_at_unix_ms,
            event: "trigger_user_message".to_string(),
            actor_kind: HistoryActorKind::User,
            actor_id: message.user_id.clone(),
            profile_ref: user_profile_ref(state, &message.user_id),
            payload: json!({
                "text": message.text,
            }),
        }
        .to_json_line(),
        pb::trigger::Kind::TaskDone(done) => {
            let status = pb::TaskStatus::try_from(done.status)
                .map(task_status_label)
                .unwrap_or("unknown");
            let result_ref = format!("task://{}/result", done.task_id);
            let result_preview = build_payload_preview(&done.result_message, result_ref);
            HistoryEventLine {
                ts_unix_ms: trigger.created_at_unix_ms,
                event: "trigger_task_done".to_string(),
                actor_kind: HistoryActorKind::Task,
                actor_id: done.task_id.clone(),
                profile_ref: active_agent_profile_ref(state),
                payload: json!({
                    "status": status,
                    "result_preview": result_preview,
                    "lookup_action": TASK_PAYLOAD_LOOKUP_ACTION,
                }),
            }
            .to_json_line()
        }
        pb::trigger::Kind::Heartbeat(_) => HistoryEventLine {
            ts_unix_ms: trigger.created_at_unix_ms,
            event: "trigger_heartbeat".to_string(),
            actor_kind: HistoryActorKind::System,
            actor_id: "runtime".to_string(),
            profile_ref: active_agent_profile_ref(state),
            payload: json!({}),
        }
        .to_json_line(),
        pb::trigger::Kind::Cron(cron) => HistoryEventLine {
            ts_unix_ms: trigger.created_at_unix_ms,
            event: "trigger_cron".to_string(),
            actor_kind: HistoryActorKind::System,
            actor_id: "runtime".to_string(),
            profile_ref: active_agent_profile_ref(state),
            payload: json!({
                "key": cron.key,
            }),
        }
        .to_json_line(),
        pb::trigger::Kind::RefreshProfile(refresh) => {
            let scope = pb::RefreshScope::try_from(refresh.scope)
                .map(refresh_scope_label)
                .unwrap_or("unknown");
            HistoryEventLine {
                ts_unix_ms: trigger.created_at_unix_ms,
                event: "trigger_refresh_profile".to_string(),
                actor_kind: HistoryActorKind::System,
                actor_id: "runtime".to_string(),
                profile_ref: active_agent_profile_ref(state),
                payload: json!({
                    "scope": scope,
                    "user_id": refresh.user_id,
                }),
            }
            .to_json_line()
        }
    }
}

pub(crate) fn assistant_output_line(
    state: &SessionState,
    ts_unix_ms: i64,
    content: &str,
) -> String {
    HistoryEventLine {
        ts_unix_ms,
        event: "assistant_output".to_string(),
        actor_kind: HistoryActorKind::Assistant,
        actor_id: state.agent_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        payload: json!({
            "content": content,
        }),
    }
    .to_json_line()
}

pub(crate) fn task_started_line(state: &SessionState, task: &pb::Task) -> String {
    let args_ref = format!("task://{}/args", task.task_id);
    let args_preview = build_payload_preview(&task.args_json, args_ref.clone());
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");
    let (environment_id, action_name) = parse_task_action_id(&task.action_id);

    HistoryEventLine {
        ts_unix_ms: task.updated_at_unix_ms,
        event: TASK_STARTED_EVENT.to_string(),
        actor_kind: HistoryActorKind::Task,
        actor_id: task.task_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        payload: json!({
            "canonical_action_id": task.action_id,
            "environment_id": environment_id,
            "action_name": action_name,
            "status": status,
            "args_preview": args_preview,
            "lookup_action": TASK_PAYLOAD_LOOKUP_ACTION,
        }),
    }
    .to_json_line()
}

pub(crate) fn task_finished_line(state: &SessionState, task: &pb::Task) -> String {
    let result_ref = format!("task://{}/result", task.task_id);
    let result_preview = build_payload_preview(&task.result_message, result_ref.clone());
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");
    let (environment_id, action_name) = parse_task_action_id(&task.action_id);

    HistoryEventLine {
        ts_unix_ms: task.updated_at_unix_ms,
        event: TASK_FINISHED_EVENT.to_string(),
        actor_kind: HistoryActorKind::Task,
        actor_id: task.task_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        payload: json!({
            "canonical_action_id": task.action_id,
            "environment_id": environment_id,
            "action_name": action_name,
            "status": status,
            "result_preview": result_preview,
            "lookup_action": TASK_PAYLOAD_LOOKUP_ACTION,
        }),
    }
    .to_json_line()
}

fn parse_task_action_id(action_id: &str) -> (String, String) {
    parse_action_id(action_id).unwrap_or_else(|| ("unknown".to_string(), action_id.to_string()))
}

fn active_agent_profile_ref(state: &SessionState) -> String {
    format!(
        "agent:{}@v{}",
        state.agent_id, state.agent_profile_copy.spec_version
    )
}

fn user_profile_ref(state: &SessionState, user_id: &str) -> String {
    let updated_at = state
        .participant_user_profiles_copy
        .get(user_id)
        .map(|profile| profile.updated_at_unix_ms)
        .unwrap_or_default();
    format!("user:{user_id}@t{updated_at}")
}
