use crate::environment::EnvironmentRegistry;
use crate::history::{PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES};
use serde::Serialize;

pub(crate) const PATH_FORMAT: &str = "relative";
pub(crate) const BASE_PATH_SCOPE: &str = "filesystem environment base_path state";
pub(crate) const ABSOLUTE_PATHS_ALLOWED: bool = false;
pub(crate) const ESCAPE_OUTSIDE_BASE_PATH_ALLOWED: bool = false;

pub(crate) const GENERAL_ACTIONS_TRIGGER_FOLLOWUP_TURN: bool = true;

pub(crate) const HISTORY_FORMAT: &str = "json_line";
pub(crate) const HISTORY_TASK_STARTED_EVENT: &str = "task_started";
pub(crate) const HISTORY_TASK_FINISHED_EVENT: &str = "task_finished";
pub(crate) const HISTORY_LOOKUP_ACTION: &str = "system__get_task_payload";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PathPolicy {
    pub(crate) path_format: String,
    pub(crate) base_path_scope: String,
    pub(crate) absolute_paths_allowed: bool,
    pub(crate) escape_outside_base_path_allowed: bool,
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

#[derive(Debug, Clone)]
pub(crate) struct PolicySnapshot {
    pub(crate) path_policy: PathPolicy,
    pub(crate) action_policy: ActionPolicy,
    pub(crate) history_policy: HistoryPolicy,
}

fn path_policy() -> PathPolicy {
    PathPolicy {
        path_format: PATH_FORMAT.to_string(),
        base_path_scope: BASE_PATH_SCOPE.to_string(),
        absolute_paths_allowed: ABSOLUTE_PATHS_ALLOWED,
        escape_outside_base_path_allowed: ESCAPE_OUTSIDE_BASE_PATH_ALLOWED,
    }
}

fn action_policy(include_actions: bool) -> ActionPolicy {
    let known_actions = if include_actions {
        EnvironmentRegistry::known_action_ids()
    } else {
        Vec::new()
    };
    ActionPolicy {
        known_actions,
        general_actions_trigger_followup_turn: GENERAL_ACTIONS_TRIGGER_FOLLOWUP_TURN,
    }
}

fn history_policy() -> HistoryPolicy {
    HistoryPolicy {
        format: HISTORY_FORMAT.to_string(),
        task_started_event: HISTORY_TASK_STARTED_EVENT.to_string(),
        task_finished_event: HISTORY_TASK_FINISHED_EVENT.to_string(),
        preview_max_bytes: PREVIEW_MAX_BYTES,
        preview_max_lines: PREVIEW_MAX_LINES,
        lookup_action: HISTORY_LOOKUP_ACTION.to_string(),
    }
}

pub(crate) fn synthesize_policy_snapshot(include_actions: bool) -> PolicySnapshot {
    PolicySnapshot {
        path_policy: path_policy(),
        action_policy: action_policy(include_actions),
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
