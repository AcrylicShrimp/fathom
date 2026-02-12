use crate::pb;

#[derive(Debug, Clone)]
pub(crate) struct SummaryBlockRefSnapshot {
    pub(crate) id: String,
    pub(crate) source_range_start: u64,
    pub(crate) source_range_end: u64,
    pub(crate) summary_text: String,
    pub(crate) created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionCompactionSnapshot {
    pub(crate) last_compacted_history_index: u64,
    pub(crate) summary_blocks: Vec<SummaryBlockRefSnapshot>,
}

#[derive(Debug, Clone)]
pub(crate) struct TurnSnapshot {
    pub(crate) session_id: String,
    pub(crate) turn_id: u64,
    pub(crate) agent_profile: pb::AgentProfile,
    pub(crate) participant_profiles: Vec<pb::UserProfile>,
    pub(crate) triggers: Vec<pb::Trigger>,
    pub(crate) recent_history: Vec<String>,
    pub(crate) compaction: SessionCompactionSnapshot,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolInvocation {
    pub(crate) tool_name: String,
    pub(crate) args_json: String,
    pub(crate) call_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StreamNote {
    pub(crate) phase: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentTurnOutcome {
    pub(crate) tool_call_count: usize,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) failed: bool,
    pub(crate) failure_code: String,
    pub(crate) failure_message: String,
}

impl AgentTurnOutcome {
    pub(crate) fn success(tool_call_count: usize, diagnostics: Vec<String>) -> Self {
        Self {
            tool_call_count,
            diagnostics,
            failed: false,
            failure_code: String::new(),
            failure_message: String::new(),
        }
    }

    pub(crate) fn failure(
        failure_code: impl Into<String>,
        failure_message: impl Into<String>,
        diagnostics: Vec<String>,
    ) -> Self {
        Self {
            tool_call_count: 0,
            diagnostics,
            failed: true,
            failure_code: failure_code.into(),
            failure_message: failure_message.into(),
        }
    }
}
