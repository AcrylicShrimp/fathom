use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::Serialize;
use serde_json::Value;

use crate::history::HistoryEvent;
use fathom_protocol::pb;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SummaryBlockRefSnapshot {
    pub(crate) id: String,
    pub(crate) source_range_start: u64,
    pub(crate) source_range_end: u64,
    pub(crate) summary_text: String,
    pub(crate) created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct SessionCompactionSnapshot {
    pub(crate) last_compacted_history_index: u64,
    pub(crate) summary_blocks: Vec<SummaryBlockRefSnapshot>,
}

#[derive(Debug, Clone)]
pub(crate) struct TurnSnapshot {
    pub(crate) session_id: String,
    pub(crate) turn_id: u64,
    pub(crate) harness_contract: HarnessContractSnapshot,
    pub(crate) identity_envelope: IdentityEnvelopeSnapshot,
    pub(crate) session_baseline: SessionBaselineSnapshot,
    pub(crate) in_flight_actions: Vec<InFlightActionHint>,
    pub(crate) resolved_payload_lookups: Vec<ResolvedPayloadLookupHint>,
    pub(crate) triggers: Vec<pb::Trigger>,
    pub(crate) recent_history: Vec<HistoryEvent>,
    pub(crate) compaction: SessionCompactionSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HarnessContractSnapshot {
    pub(crate) runtime_version: String,
    pub(crate) contract_schema_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct IdentityEnvelopeSnapshot {
    pub(crate) schema_version: u32,
    pub(crate) source_revision: String,
    pub(crate) material: Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionBaselineSnapshot {
    pub(crate) session_anchor: SessionAnchorSnapshot,
    pub(crate) capability_surface: CapabilitySurfaceSnapshot,
    pub(crate) participant_envelope: ParticipantEnvelopeSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionAnchorSnapshot {
    pub(crate) session_id: String,
    pub(crate) started_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilitySurfaceSnapshot {
    pub(crate) environments: Vec<CapabilityEnvironmentSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityEnvironmentSnapshot {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) actions: Vec<CapabilityActionSnapshot>,
    pub(crate) recipes: Vec<CapabilityRecipeSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionModeSupportSnapshot {
    AwaitOnly,
    AwaitOrDetach,
}

impl ActionModeSupportSnapshot {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AwaitOnly => "await_only",
            Self::AwaitOrDetach => "await_or_detach",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityActionSnapshot {
    pub(crate) action_id: String,
    pub(crate) description: String,
    pub(crate) mode_support: ActionModeSupportSnapshot,
    pub(crate) discovery: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityRecipeSnapshot {
    pub(crate) title: String,
    pub(crate) steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ParticipantEnvelopeSnapshot {
    pub(crate) schema_version: u32,
    pub(crate) source_revision: String,
    pub(crate) material: Value,
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InFlightActionHint {
    pub(crate) execution_id: String,
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) env_seq: u64,
    pub(crate) status: String,
    pub(crate) submitted_at_unix_ms: i64,
    pub(crate) args_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ResolvedPayloadLookupHint {
    pub(crate) lookup_execution_id: String,
    pub(crate) execution_id: String,
    pub(crate) part: String,
    pub(crate) offset: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) full_bytes: usize,
    pub(crate) source_truncated: bool,
    pub(crate) payload_chunk: String,
    pub(crate) injected_truncated: bool,
    pub(crate) injected_omitted_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptMessage {
    pub(crate) role: String,
    pub(crate) label: String,
    pub(crate) content: String,
    pub(crate) stable_hash: String,
}

impl PromptMessage {
    pub(crate) fn new(role: impl Into<String>, label: impl Into<String>, content: String) -> Self {
        let role = role.into();
        let label = label.into();
        let mut hasher = DefaultHasher::new();
        role.hash(&mut hasher);
        label.hash(&mut hasher);
        content.hash(&mut hasher);
        let stable_hash = format!("{:016x}", hasher.finish());
        Self {
            role,
            label,
            content,
            stable_hash,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptMessageStat {
    pub(crate) label: String,
    pub(crate) role: String,
    pub(crate) estimated_tokens: usize,
    pub(crate) stable_hash: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct PromptBuildStats {
    pub(crate) estimated_prompt_tokens: usize,
    pub(crate) messages_count: usize,
    pub(crate) stable_prefix_hash: String,
    pub(crate) compaction_applied: bool,
    pub(crate) compaction_reason: String,
    pub(crate) timeline_raw_events: usize,
    pub(crate) timeline_compacted_events: usize,
    pub(crate) dedup_dropped_events: usize,
    pub(crate) per_message: Vec<PromptMessageStat>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct PromptMessageBundle {
    pub(crate) messages: Vec<PromptMessage>,
    pub(crate) stats: PromptBuildStats,
}

impl PromptMessageBundle {
    pub(crate) fn as_debug_prompt(&self) -> String {
        let mut sections = Vec::with_capacity(self.messages.len());
        for message in &self.messages {
            sections.push(format!(
                "### {} ({})\n{}",
                message.label, message.role, message.content
            ));
        }
        sections.join("\n\n")
    }
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
pub(crate) enum ModelDeltaEvent {
    StreamNote(StreamNote),
    ActionInvocation(ActionInvocation),
    ActionArgsDelta(ActionArgDeltaNote),
    ActionArgsDone(ActionArgDoneNote),
    AssistantTextDelta(String),
    AssistantTextDone(String),
}

#[derive(Debug, Clone)]
pub(crate) struct ModelInvocationOutcome {
    pub(crate) action_call_count: usize,
    pub(crate) assistant_outputs: Vec<String>,
    pub(crate) diagnostics: Vec<String>,
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
