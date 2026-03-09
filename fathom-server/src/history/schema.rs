use serde::Serialize;

use crate::history::preview::PayloadPreview;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistoryActorKind {
    User,
    Assistant,
    System,
    Execution,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryEvent {
    pub(crate) ts_unix_ms: i64,
    pub(crate) actor_kind: HistoryActorKind,
    pub(crate) actor_id: String,
    pub(crate) profile_ref: String,
    #[serde(flatten)]
    pub(crate) kind: HistoryEventKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
pub(crate) enum HistoryEventKind {
    #[serde(rename = "trigger_unknown")]
    TriggerUnknown,
    #[serde(rename = "trigger_user_message")]
    TriggerUserMessage(UserMessageHistoryPayload),
    #[serde(rename = "execution_requested")]
    ExecutionRequested(ExecutionRequestedHistoryPayload),
    #[serde(rename = "awaited_execution_succeeded")]
    AwaitedExecutionSucceeded(ExecutionSucceededHistoryPayload),
    #[serde(rename = "awaited_execution_failed")]
    AwaitedExecutionFailed(ExecutionFailedHistoryPayload),
    #[serde(rename = "execution_detached")]
    ExecutionDetached(ExecutionDetachedHistoryPayload),
    #[serde(rename = "detached_execution_succeeded")]
    DetachedExecutionSucceeded(ExecutionSucceededHistoryPayload),
    #[serde(rename = "detached_execution_failed")]
    DetachedExecutionFailed(ExecutionFailedHistoryPayload),
    #[serde(rename = "execution_rejected")]
    ExecutionRejected(ExecutionRejectedHistoryPayload),
    #[serde(rename = "trigger_heartbeat")]
    TriggerHeartbeat,
    #[serde(rename = "trigger_cron")]
    TriggerCron(CronHistoryPayload),
    #[serde(rename = "trigger_refresh_profile")]
    TriggerRefreshProfile(RefreshProfileHistoryPayload),
    #[serde(rename = "assistant_output")]
    AssistantOutput(AssistantOutputHistoryPayload),
}

impl HistoryEventKind {
    pub(crate) fn summary_group(&self) -> &'static str {
        match self {
            Self::TriggerUnknown => "other",
            Self::TriggerUserMessage(_) => "user_message",
            Self::ExecutionRequested(_) => "execution_requested",
            Self::AwaitedExecutionSucceeded(_) => "awaited_execution_succeeded",
            Self::AwaitedExecutionFailed(_) => "awaited_execution_failed",
            Self::ExecutionDetached(_) => "execution_detached",
            Self::DetachedExecutionSucceeded(_) => "detached_execution_succeeded",
            Self::DetachedExecutionFailed(_) => "detached_execution_failed",
            Self::ExecutionRejected(_) => "execution_rejected",
            Self::TriggerHeartbeat => "heartbeat",
            Self::TriggerCron(_) => "cron",
            Self::TriggerRefreshProfile(_) => "refresh_profile",
            Self::AssistantOutput(_) => "assistant_output",
        }
    }

    pub(crate) fn status(&self) -> Option<&str> {
        match self {
            Self::ExecutionRequested(payload) => Some(&payload.status),
            _ => None,
        }
    }

    pub(crate) fn canonical_action_id(&self) -> Option<&str> {
        match self {
            Self::ExecutionRequested(payload) => Some(&payload.canonical_action_id),
            Self::AwaitedExecutionSucceeded(payload)
            | Self::DetachedExecutionSucceeded(payload) => Some(&payload.canonical_action_id),
            Self::AwaitedExecutionFailed(payload) | Self::DetachedExecutionFailed(payload) => {
                Some(&payload.canonical_action_id)
            }
            Self::ExecutionDetached(payload) => Some(&payload.canonical_action_id),
            Self::ExecutionRejected(payload) => Some(&payload.canonical_action_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UserMessageHistoryPayload {
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CronHistoryPayload {
    pub(crate) key: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RefreshProfileHistoryPayload {
    pub(crate) scope: String,
    pub(crate) user_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssistantOutputHistoryPayload {
    pub(crate) content: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExecutionRequestedHistoryPayload {
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) execution_mode: String,
    pub(crate) status: String,
    pub(crate) args_preview: PayloadPreview,
    pub(crate) lookup_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExecutionSucceededHistoryPayload {
    pub(crate) canonical_action_id: String,
    pub(crate) payload_preview: PayloadPreview,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExecutionFailedHistoryPayload {
    pub(crate) canonical_action_id: String,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) payload_preview: Option<PayloadPreview>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExecutionDetachedHistoryPayload {
    pub(crate) canonical_action_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExecutionRejectedHistoryPayload {
    pub(crate) canonical_action_id: String,
    pub(crate) message: String,
}
