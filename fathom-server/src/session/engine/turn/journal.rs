use crate::agent::{PromptMessageBundle, TurnSnapshot};
use crate::runtime::Runtime;
use crate::session::diagnostics::{trigger_to_json, turn_snapshot_to_json};
use crate::session::state::SessionState;
use crate::util::now_unix_ms;

use super::types::AgentTurnSummary;

pub(super) fn append_turn_started_record(
    runtime: &Runtime,
    state: &SessionState,
    turn_id: u64,
    turn_triggers: &[crate::pb::Trigger],
) {
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "turn.started",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "trigger_count": turn_triggers.len(),
            "triggers": turn_triggers.iter().map(trigger_to_json).collect::<Vec<_>>(),
        }),
    );
}

pub(super) fn write_invocation_context(
    runtime: &Runtime,
    state: &SessionState,
    turn_id: u64,
    invocation_seq: u64,
    snapshot: &TurnSnapshot,
    prompt_bundle: &PromptMessageBundle,
) {
    runtime.diagnostics().write_invocation_context(
        &state.session_id,
        invocation_seq,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "agent.invocation.context",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "invocation_seq": invocation_seq,
            "snapshot": turn_snapshot_to_json(snapshot),
            "prompt": prompt_bundle.as_debug_prompt(),
            "prompt_messages": prompt_bundle.messages.clone(),
            "prompt_stats": prompt_bundle.stats.clone(),
        }),
    );
}

pub(super) fn append_invocation_started_record(
    runtime: &Runtime,
    state: &SessionState,
    turn_id: u64,
    invocation_seq: u64,
) {
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "agent.invocation.started",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "invocation_seq": invocation_seq,
            "context_path": invocation_detail_path(state, invocation_seq),
        }),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_invocation_finished_record(
    runtime: &Runtime,
    state: &SessionState,
    turn_id: u64,
    invocation_seq: u64,
    failed: bool,
    failure_code: &str,
    failure_message: &str,
    action_call_count: usize,
    assistant_outputs: &[String],
    diagnostics: &[String],
    stream_notes: &[serde_json::Value],
    action_dispatches: &[serde_json::Value],
) {
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "agent.invocation.finished",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "invocation_seq": invocation_seq,
            "failed": failed,
            "failure_code": failure_code,
            "failure_message": failure_message,
            "action_call_count": action_call_count,
            "assistant_outputs": assistant_outputs,
            "diagnostics": diagnostics,
            "stream_notes": stream_notes,
            "action_dispatches": action_dispatches,
        }),
    );
}

pub(super) fn append_turn_ended_record(
    runtime: &Runtime,
    state: &SessionState,
    turn_id: u64,
    agent_summary: Option<AgentTurnSummary>,
    is_quiescent: bool,
) {
    runtime.diagnostics().append_session_record(
        &state.session_id,
        serde_json::json!({
            "ts_unix_ms": now_unix_ms(),
            "event": "turn.ended",
            "session_id": state.session_id,
            "turn_id": turn_id,
            "history_size": state.history.len(),
            "pending_trigger_count": state.trigger_queue.len(),
            "in_flight_action_count": state.in_flight_actions.len(),
            "agent_summary": agent_summary.map(|summary| serde_json::json!({
                "action_call_count": summary.action_call_count,
                "assistant_output_count": summary.assistant_output_count,
            })),
            "quiescent": is_quiescent,
        }),
    );
}

fn invocation_detail_path(state: &SessionState, invocation_seq: u64) -> String {
    format!(
        "sessions/{}/invocations/invocation-{}.json",
        state.session_id, invocation_seq
    )
}
