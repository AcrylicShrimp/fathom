use fathom_env::parse_action_id;

use crate::history::TASK_PAYLOAD_LOOKUP_ACTION;
use crate::history::preview::build_payload_preview;
use crate::history::schema::{
    AssistantOutputHistoryPayload, CronHistoryPayload, HistoryActorKind, HistoryEvent,
    HistoryEventKind, RefreshProfileHistoryPayload, TaskDoneHistoryPayload,
    TaskFinishedHistoryPayload, TaskStartedHistoryPayload, UserMessageHistoryPayload,
};
use crate::pb;
use crate::session::state::SessionState;
use crate::util::{refresh_scope_label, task_status_label};

pub(crate) fn trigger_line(state: &SessionState, trigger: &pb::Trigger) -> HistoryEvent {
    let Some(kind) = trigger.kind.as_ref() else {
        return HistoryEvent {
            ts_unix_ms: trigger.created_at_unix_ms,
            actor_kind: HistoryActorKind::System,
            actor_id: "runtime".to_string(),
            profile_ref: active_agent_profile_ref(state),
            kind: HistoryEventKind::TriggerUnknown,
        };
    };

    match kind {
        pb::trigger::Kind::UserMessage(message) => HistoryEvent {
            ts_unix_ms: trigger.created_at_unix_ms,
            actor_kind: HistoryActorKind::User,
            actor_id: message.user_id.clone(),
            profile_ref: user_profile_ref(state, &message.user_id),
            kind: HistoryEventKind::TriggerUserMessage(UserMessageHistoryPayload {
                text: message.text.clone(),
            }),
        },
        pb::trigger::Kind::TaskDone(done) => {
            let status = pb::TaskStatus::try_from(done.status)
                .map(task_status_label)
                .unwrap_or("unknown");
            let result_ref = format!("task://{}/result", done.task_id);
            let result_preview = build_payload_preview(&done.result_message, result_ref);
            HistoryEvent {
                ts_unix_ms: trigger.created_at_unix_ms,
                actor_kind: HistoryActorKind::Task,
                actor_id: done.task_id.clone(),
                profile_ref: active_agent_profile_ref(state),
                kind: HistoryEventKind::TriggerTaskDone(TaskDoneHistoryPayload {
                    status: status.to_string(),
                    result_preview,
                    lookup_action: TASK_PAYLOAD_LOOKUP_ACTION.to_string(),
                }),
            }
        }
        pb::trigger::Kind::Heartbeat(_) => HistoryEvent {
            ts_unix_ms: trigger.created_at_unix_ms,
            actor_kind: HistoryActorKind::System,
            actor_id: "runtime".to_string(),
            profile_ref: active_agent_profile_ref(state),
            kind: HistoryEventKind::TriggerHeartbeat,
        },
        pb::trigger::Kind::Cron(cron) => HistoryEvent {
            ts_unix_ms: trigger.created_at_unix_ms,
            actor_kind: HistoryActorKind::System,
            actor_id: "runtime".to_string(),
            profile_ref: active_agent_profile_ref(state),
            kind: HistoryEventKind::TriggerCron(CronHistoryPayload {
                key: cron.key.clone(),
            }),
        },
        pb::trigger::Kind::RefreshProfile(refresh) => {
            let scope = pb::RefreshScope::try_from(refresh.scope)
                .map(refresh_scope_label)
                .unwrap_or("unknown");
            HistoryEvent {
                ts_unix_ms: trigger.created_at_unix_ms,
                actor_kind: HistoryActorKind::System,
                actor_id: "runtime".to_string(),
                profile_ref: active_agent_profile_ref(state),
                kind: HistoryEventKind::TriggerRefreshProfile(RefreshProfileHistoryPayload {
                    scope: scope.to_string(),
                    user_id: refresh.user_id.clone(),
                }),
            }
        }
    }
}

pub(crate) fn assistant_output_line(
    state: &SessionState,
    ts_unix_ms: i64,
    content: &str,
) -> HistoryEvent {
    HistoryEvent {
        ts_unix_ms,
        actor_kind: HistoryActorKind::Assistant,
        actor_id: state.agent_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        kind: HistoryEventKind::AssistantOutput(AssistantOutputHistoryPayload {
            content: content.to_string(),
        }),
    }
}

pub(crate) fn task_started_line(state: &SessionState, task: &pb::Task) -> HistoryEvent {
    let args_ref = format!("task://{}/args", task.task_id);
    let args_preview = build_payload_preview(&task.args_json, args_ref);
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");
    let (environment_id, action_name) = parse_task_action_id(&task.action_id);

    HistoryEvent {
        ts_unix_ms: task.updated_at_unix_ms,
        actor_kind: HistoryActorKind::Task,
        actor_id: task.task_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        kind: HistoryEventKind::TaskStarted(TaskStartedHistoryPayload {
            canonical_action_id: task.action_id.clone(),
            environment_id,
            action_name,
            status: status.to_string(),
            args_preview,
            lookup_action: TASK_PAYLOAD_LOOKUP_ACTION.to_string(),
        }),
    }
}

pub(crate) fn task_finished_line(state: &SessionState, task: &pb::Task) -> HistoryEvent {
    let result_ref = format!("task://{}/result", task.task_id);
    let result_preview = build_payload_preview(&task.result_message, result_ref);
    let status = pb::TaskStatus::try_from(task.status)
        .map(task_status_label)
        .unwrap_or("unknown");
    let (environment_id, action_name) = parse_task_action_id(&task.action_id);

    HistoryEvent {
        ts_unix_ms: task.updated_at_unix_ms,
        actor_kind: HistoryActorKind::Task,
        actor_id: task.task_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        kind: HistoryEventKind::TaskFinished(TaskFinishedHistoryPayload {
            canonical_action_id: task.action_id.clone(),
            environment_id,
            action_name,
            status: status.to_string(),
            result_preview,
            lookup_action: TASK_PAYLOAD_LOOKUP_ACTION.to_string(),
        }),
    }
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
