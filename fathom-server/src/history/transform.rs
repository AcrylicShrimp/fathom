use fathom_capability_domain::parse_action_id;

use crate::history::EXECUTION_INPUT_LOOKUP_ACTION;
use crate::history::preview::build_payload_preview;
use crate::history::schema::{
    AssistantOutputHistoryPayload, CronHistoryPayload, ExecutionBackgroundedHistoryPayload,
    ExecutionCanceledHistoryPayload, ExecutionFailedHistoryPayload,
    ExecutionRejectedHistoryPayload, ExecutionRequestedHistoryPayload,
    ExecutionSucceededHistoryPayload, HistoryActorKind, HistoryEvent, HistoryEventKind,
    RefreshProfileHistoryPayload, UserMessageHistoryPayload,
};
use crate::session::state::SessionState;
use fathom_protocol::pb;
use fathom_protocol::{execution_status_label, refresh_scope_label};

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
        pb::trigger::Kind::ExecutionUpdate(update) => HistoryEvent {
            ts_unix_ms: trigger.created_at_unix_ms,
            actor_kind: HistoryActorKind::Execution,
            actor_id: update.execution_id.clone(),
            profile_ref: active_agent_profile_ref(state),
            kind: execution_update_history_kind(update),
        },
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

pub(crate) fn execution_requested_line(
    state: &SessionState,
    execution: &pb::Execution,
) -> HistoryEvent {
    let args_ref = format!("execution://{}/args", execution.execution_id);
    let args_preview = build_payload_preview(&execution.args_json, args_ref);
    let status = pb::ExecutionStatus::try_from(execution.status)
        .map(execution_status_label)
        .unwrap_or("unknown");
    let (capability_domain_id, action_name) = parse_action_identity(&execution.action_id);
    let background = background_requested_from_args_json(&execution.args_json);

    HistoryEvent {
        ts_unix_ms: execution.updated_at_unix_ms,
        actor_kind: HistoryActorKind::Execution,
        actor_id: execution.execution_id.clone(),
        profile_ref: active_agent_profile_ref(state),
        kind: HistoryEventKind::ExecutionRequested(ExecutionRequestedHistoryPayload {
            canonical_action_id: execution.action_id.clone(),
            capability_domain_id,
            action_name,
            background,
            status: status.to_string(),
            args_preview,
            lookup_action: EXECUTION_INPUT_LOOKUP_ACTION.to_string(),
        }),
    }
}

fn parse_action_identity(action_id: &str) -> (String, String) {
    parse_action_id(action_id).unwrap_or_else(|| ("unknown".to_string(), action_id.to_string()))
}

fn background_requested_from_args_json(args_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(args_json)
        .ok()
        .and_then(|value| value.get("background").cloned())
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn execution_update_history_kind(update: &pb::ExecutionUpdateTrigger) -> HistoryEventKind {
    let payload_preview = if update.payload_message.trim().is_empty() {
        None
    } else {
        Some(build_payload_preview(
            &update.payload_message,
            format!("execution://{}/result", update.execution_id),
        ))
    };
    let kind = pb::ExecutionUpdateKind::try_from(update.kind)
        .unwrap_or(pb::ExecutionUpdateKind::Unspecified);

    match kind {
        pb::ExecutionUpdateKind::ExecutionSucceeded => {
            HistoryEventKind::ExecutionSucceeded(ExecutionSucceededHistoryPayload {
                canonical_action_id: update.action_id.clone(),
                payload_preview: payload_preview.unwrap_or_else(|| {
                    build_payload_preview("", format!("execution://{}/result", update.execution_id))
                }),
            })
        }
        pb::ExecutionUpdateKind::ExecutionFailed => {
            HistoryEventKind::ExecutionFailed(ExecutionFailedHistoryPayload {
                canonical_action_id: update.action_id.clone(),
                message: update.message.clone(),
                payload_preview,
            })
        }
        pb::ExecutionUpdateKind::ExecutionBackgrounded => {
            HistoryEventKind::ExecutionBackgrounded(ExecutionBackgroundedHistoryPayload {
                canonical_action_id: update.action_id.clone(),
            })
        }
        pb::ExecutionUpdateKind::ExecutionCanceled => {
            HistoryEventKind::ExecutionCanceled(ExecutionCanceledHistoryPayload {
                canonical_action_id: update.action_id.clone(),
            })
        }
        pb::ExecutionUpdateKind::ExecutionRejected | pb::ExecutionUpdateKind::Unspecified => {
            HistoryEventKind::ExecutionRejected(ExecutionRejectedHistoryPayload {
                canonical_action_id: update.action_id.clone(),
                message: update.message.clone(),
            })
        }
    }
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
