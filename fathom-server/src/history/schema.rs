use serde::Serialize;

use crate::history::preview::PayloadPreview;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistoryActorKind {
    User,
    Assistant,
    System,
    Task,
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
    #[serde(rename = "trigger_task_done")]
    TriggerTaskDone(TaskDoneHistoryPayload),
    #[serde(rename = "trigger_heartbeat")]
    TriggerHeartbeat,
    #[serde(rename = "trigger_cron")]
    TriggerCron(CronHistoryPayload),
    #[serde(rename = "trigger_refresh_profile")]
    TriggerRefreshProfile(RefreshProfileHistoryPayload),
    #[serde(rename = "assistant_output")]
    AssistantOutput(AssistantOutputHistoryPayload),
    #[serde(rename = "task_started")]
    TaskStarted(TaskStartedHistoryPayload),
    #[serde(rename = "task_finished")]
    TaskFinished(TaskFinishedHistoryPayload),
}

impl HistoryEventKind {
    pub(crate) fn summary_group(&self) -> &'static str {
        match self {
            Self::TriggerUnknown => "other",
            Self::TriggerUserMessage(_) => "user_message",
            Self::TriggerTaskDone(_) => "task_done",
            Self::TriggerHeartbeat => "heartbeat",
            Self::TriggerCron(_) => "cron",
            Self::TriggerRefreshProfile(_) => "refresh_profile",
            Self::AssistantOutput(_) => "assistant_output",
            Self::TaskStarted(_) => "task_started",
            Self::TaskFinished(_) => "task_finished",
        }
    }

    pub(crate) fn status(&self) -> Option<&str> {
        match self {
            Self::TriggerTaskDone(payload) => Some(&payload.status),
            Self::TaskStarted(payload) => Some(&payload.status),
            Self::TaskFinished(payload) => Some(&payload.status),
            _ => None,
        }
    }

    pub(crate) fn canonical_action_id(&self) -> Option<&str> {
        match self {
            Self::TaskStarted(payload) => Some(&payload.canonical_action_id),
            Self::TaskFinished(payload) => Some(&payload.canonical_action_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UserMessageHistoryPayload {
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskDoneHistoryPayload {
    pub(crate) status: String,
    pub(crate) result_preview: PayloadPreview,
    pub(crate) lookup_action: String,
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
pub(crate) struct TaskStartedHistoryPayload {
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) status: String,
    pub(crate) args_preview: PayloadPreview,
    pub(crate) lookup_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskFinishedHistoryPayload {
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) status: String,
    pub(crate) result_preview: PayloadPreview,
    pub(crate) lookup_action: String,
}
