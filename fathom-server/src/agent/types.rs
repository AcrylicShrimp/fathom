use std::collections::BTreeMap;

use serde::Serialize;

use crate::pb;
use crate::policy::{ActionPolicy, EnvironmentPolicy, HistoryPolicy, PathPolicy};

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
    pub(crate) system_context: SystemContextSnapshot,
    pub(crate) agent_profile: pb::AgentProfile,
    pub(crate) participant_profiles: Vec<pb::UserProfile>,
    pub(crate) triggers: Vec<pb::Trigger>,
    pub(crate) recent_history: Vec<String>,
    pub(crate) compaction: SessionCompactionSnapshot,
}

#[derive(Debug, Clone)]
pub(crate) struct SystemContextSnapshot {
    pub(crate) runtime_version: String,
    pub(crate) workspace_root: String,
    pub(crate) time_context: SystemTimeContext,
    pub(crate) path_policy: PathPolicy,
    pub(crate) session_identity: SessionIdentityMapSnapshot,
    pub(crate) action_policy: ActionPolicy,
    pub(crate) environment_policy: EnvironmentPolicy,
    pub(crate) history_policy: HistoryPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SystemTimeContext {
    pub(crate) generated_at_unix_ms: i64,
    pub(crate) utc_rfc3339: String,
    pub(crate) local_rfc3339: String,
    pub(crate) local_timezone_name: String,
    pub(crate) local_utc_offset: String,
    pub(crate) time_source: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionIdentityMapSnapshot {
    pub(crate) session_id: String,
    pub(crate) active_agent_id: String,
    pub(crate) participant_user_ids: Vec<String>,
    pub(crate) active_agent_spec_version: u64,
    pub(crate) participant_user_updated_at: BTreeMap<String, i64>,
    pub(crate) engaged_environment_ids: Vec<String>,
    pub(crate) in_flight_actions: Vec<InFlightActionHint>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InFlightActionHint {
    pub(crate) task_id: String,
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) env_seq: u64,
    pub(crate) status: String,
    pub(crate) submitted_at_unix_ms: i64,
    pub(crate) args_preview: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ActionInvocation {
    pub(crate) action_id: String,
    pub(crate) args_json: String,
    pub(crate) call_key: String,
    pub(crate) call_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StreamNote {
    pub(crate) phase: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ActionArgDeltaNote {
    pub(crate) call_key: String,
    pub(crate) call_id: Option<String>,
    pub(crate) action_id: Option<String>,
    pub(crate) args_delta: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ActionArgDoneNote {
    pub(crate) call_key: String,
    pub(crate) call_id: Option<String>,
    pub(crate) action_id: Option<String>,
    pub(crate) args_json: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentTurnOutcome {
    pub(crate) action_call_count: usize,
    pub(crate) assistant_outputs: Vec<String>,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) failed: bool,
    pub(crate) failure_code: String,
    pub(crate) failure_message: String,
}

impl AgentTurnOutcome {
    pub(crate) fn success(
        action_call_count: usize,
        assistant_outputs: Vec<String>,
        diagnostics: Vec<String>,
    ) -> Self {
        Self {
            action_call_count,
            assistant_outputs,
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
            action_call_count: 0,
            assistant_outputs: Vec::new(),
            diagnostics,
            failed: true,
            failure_code: failure_code.into(),
            failure_message: failure_message.into(),
        }
    }
}
