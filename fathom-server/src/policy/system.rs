use crate::environment::EnvironmentRegistry;
use crate::history::{PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES};
use serde::Serialize;

pub(crate) const MANAGED_URI_PATTERNS: [&str; 2] = [
    "managed://agent/<agent_id>/<field>",
    "managed://user/<user_id>/<field>",
];
pub(crate) const FS_URI_POLICY: &str = "workspace-relative only";

pub(crate) const GENERAL_ACTIONS_TRIGGER_FOLLOWUP_TURN: bool = true;

pub(crate) const HISTORY_FORMAT: &str = "json_line";
pub(crate) const HISTORY_TASK_STARTED_EVENT: &str = "task_started";
pub(crate) const HISTORY_TASK_FINISHED_EVENT: &str = "task_finished";
pub(crate) const HISTORY_LOOKUP_ACTION: &str = "system__get_task_payload";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PathPolicy {
    pub(crate) managed_uri_patterns: Vec<String>,
    pub(crate) fs_uri_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ActionPolicy {
    pub(crate) known_actions: Vec<String>,
    pub(crate) general_actions_trigger_followup_turn: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryPolicy {
    pub(crate) format: String,
    pub(crate) task_started_event: String,
    pub(crate) task_finished_event: String,
    pub(crate) preview_max_bytes: usize,
    pub(crate) preview_max_lines: usize,
    pub(crate) lookup_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EnvironmentPolicy {
    pub(crate) default_engaged_environments: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SystemPolicy {
    pub(crate) path_policy: PathPolicy,
    pub(crate) action_policy: ActionPolicy,
    pub(crate) environment_policy: EnvironmentPolicy,
    pub(crate) history_policy: HistoryPolicy,
}

pub(crate) fn path_policy() -> PathPolicy {
    PathPolicy {
        managed_uri_patterns: MANAGED_URI_PATTERNS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        fs_uri_policy: FS_URI_POLICY.to_string(),
    }
}

pub(crate) fn action_policy() -> ActionPolicy {
    ActionPolicy {
        known_actions: EnvironmentRegistry::known_action_ids(),
        general_actions_trigger_followup_turn: GENERAL_ACTIONS_TRIGGER_FOLLOWUP_TURN,
    }
}

pub(crate) fn history_policy() -> HistoryPolicy {
    HistoryPolicy {
        format: HISTORY_FORMAT.to_string(),
        task_started_event: HISTORY_TASK_STARTED_EVENT.to_string(),
        task_finished_event: HISTORY_TASK_FINISHED_EVENT.to_string(),
        preview_max_bytes: PREVIEW_MAX_BYTES,
        preview_max_lines: PREVIEW_MAX_LINES,
        lookup_action: HISTORY_LOOKUP_ACTION.to_string(),
    }
}

pub(crate) fn environment_policy() -> EnvironmentPolicy {
    EnvironmentPolicy {
        default_engaged_environments: EnvironmentRegistry::default_engaged_environment_ids(),
    }
}

pub(crate) fn system_policy() -> SystemPolicy {
    SystemPolicy {
        path_policy: path_policy(),
        action_policy: action_policy(),
        environment_policy: environment_policy(),
        history_policy: history_policy(),
    }
}

pub(crate) fn history_task_started_event() -> &'static str {
    HISTORY_TASK_STARTED_EVENT
}

pub(crate) fn history_task_finished_event() -> &'static str {
    HISTORY_TASK_FINISHED_EVENT
}

pub(crate) fn history_lookup_action() -> &'static str {
    HISTORY_LOOKUP_ACTION
}
