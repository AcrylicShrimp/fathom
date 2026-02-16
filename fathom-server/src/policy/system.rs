use crate::history::{PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES};
use serde::Serialize;

pub(crate) const MANAGED_URI_PATTERNS: [&str; 2] = [
    "managed://agent/<agent_id>/<field>",
    "managed://user/<user_id>/<field>",
];
pub(crate) const FS_URI_POLICY: &str = "workspace-relative only";

pub(crate) const NON_TRIGGERING_TOOLS: [&str; 1] = ["send_message"];
pub(crate) const GENERAL_TOOLS_TRIGGER_FOLLOWUP_TURN: bool = true;

pub(crate) const HISTORY_FORMAT: &str = "json_line";
pub(crate) const HISTORY_TASK_STARTED_EVENT: &str = "task_started";
pub(crate) const HISTORY_TASK_FINISHED_EVENT: &str = "task_finished";
pub(crate) const HISTORY_LOOKUP_TOOL: &str = "sys_get_task_payload";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PathPolicy {
    pub(crate) managed_uri_patterns: Vec<String>,
    pub(crate) fs_uri_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ToolPolicy {
    pub(crate) non_triggering_tools: Vec<String>,
    pub(crate) general_tools_trigger_followup_turn: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryPolicy {
    pub(crate) format: String,
    pub(crate) task_started_event: String,
    pub(crate) task_finished_event: String,
    pub(crate) preview_max_bytes: usize,
    pub(crate) preview_max_lines: usize,
    pub(crate) lookup_tool: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SystemPolicy {
    pub(crate) path_policy: PathPolicy,
    pub(crate) tool_policy: ToolPolicy,
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

pub(crate) fn tool_policy() -> ToolPolicy {
    ToolPolicy {
        non_triggering_tools: NON_TRIGGERING_TOOLS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        general_tools_trigger_followup_turn: GENERAL_TOOLS_TRIGGER_FOLLOWUP_TURN,
    }
}

pub(crate) fn history_policy() -> HistoryPolicy {
    HistoryPolicy {
        format: HISTORY_FORMAT.to_string(),
        task_started_event: HISTORY_TASK_STARTED_EVENT.to_string(),
        task_finished_event: HISTORY_TASK_FINISHED_EVENT.to_string(),
        preview_max_bytes: PREVIEW_MAX_BYTES,
        preview_max_lines: PREVIEW_MAX_LINES,
        lookup_tool: HISTORY_LOOKUP_TOOL.to_string(),
    }
}

pub(crate) fn system_policy() -> SystemPolicy {
    SystemPolicy {
        path_policy: path_policy(),
        tool_policy: tool_policy(),
        history_policy: history_policy(),
    }
}

pub(crate) fn history_task_started_event() -> &'static str {
    HISTORY_TASK_STARTED_EVENT
}

pub(crate) fn history_task_finished_event() -> &'static str {
    HISTORY_TASK_FINISHED_EVENT
}

pub(crate) fn history_lookup_tool() -> &'static str {
    HISTORY_LOOKUP_TOOL
}
