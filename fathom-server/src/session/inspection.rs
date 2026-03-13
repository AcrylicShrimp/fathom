use fathom_protocol::pb;
use serde_json::Value;

use crate::session::state::{ExecutionSubmissionStatus, SessionState};

pub(crate) const DEFAULT_EXECUTION_LIST_LIMIT: usize = 20;
pub(crate) const MAX_EXECUTION_LIST_LIMIT: usize = 100;
pub(crate) const MAX_EXECUTION_PAYLOAD_LIMIT: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutionInspectionState {
    Queued,
    RunningForeground,
    RunningBackground,
    Succeeded,
    Failed,
    Canceled,
}

impl ExecutionInspectionState {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::RunningForeground => "running_foreground",
            Self::RunningBackground => "running_background",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "queued" => Some(Self::Queued),
            "running_foreground" => Some(Self::RunningForeground),
            "running_background" => Some(Self::RunningBackground),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "canceled" => Some(Self::Canceled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionListQuery {
    pub(crate) cursor: Option<String>,
    pub(crate) limit: usize,
    pub(crate) state: Option<ExecutionInspectionState>,
    pub(crate) action_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionSummary {
    pub(crate) execution_id: String,
    pub(crate) action_id: String,
    pub(crate) state: ExecutionInspectionState,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionListPage {
    pub(crate) executions: Vec<ExecutionSummary>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) prev_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionInspection {
    pub(crate) execution_id: String,
    pub(crate) action_id: String,
    pub(crate) state: ExecutionInspectionState,
    pub(crate) input_payload: String,
    pub(crate) result_payload: Option<String>,
    pub(crate) execution_time_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct PayloadSlice {
    pub(crate) total_size: usize,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) content: String,
}

pub(crate) fn list_executions(
    state: &SessionState,
    query: &ExecutionListQuery,
) -> Result<ExecutionListPage, String> {
    let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
    let limit = normalize_execution_list_limit(query.limit);

    let mut rows = state
        .executions
        .values()
        .filter_map(|execution| {
            let summary = summarize_execution(state, execution)?;
            if let Some(expected_state) = &query.state
                && &summary.state != expected_state
            {
                return None;
            }
            if let Some(expected_action_id) = query.action_id.as_deref()
                && summary.action_id != expected_action_id
            {
                return None;
            }
            Some(ExecutionRow {
                order: execution_order(execution),
                summary,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .order
            .cmp(&left.order)
            .then_with(|| right.summary.execution_id.cmp(&left.summary.execution_id))
    });

    let filtered_rows = match cursor {
        Some(CursorBoundary::Before(order)) => rows
            .iter()
            .filter(|row| row.order < order)
            .cloned()
            .collect::<Vec<_>>(),
        Some(CursorBoundary::After(order)) => rows
            .iter()
            .filter(|row| row.order > order)
            .cloned()
            .collect::<Vec<_>>(),
        None => rows.clone(),
    };
    let page_rows = filtered_rows.into_iter().take(limit).collect::<Vec<_>>();

    let next_cursor = page_rows.last().and_then(|row| {
        rows.iter()
            .any(|candidate| candidate.order < row.order)
            .then(|| encode_cursor(CursorBoundary::Before(row.order)))
    });
    let prev_cursor = page_rows.first().and_then(|row| {
        rows.iter()
            .any(|candidate| candidate.order > row.order)
            .then(|| encode_cursor(CursorBoundary::After(row.order)))
    });

    Ok(ExecutionListPage {
        executions: page_rows.into_iter().map(|row| row.summary).collect(),
        next_cursor,
        prev_cursor,
    })
}

pub(crate) fn get_execution(
    state: &SessionState,
    execution_id: &str,
) -> Option<ExecutionInspection> {
    let execution = state.executions.get(execution_id)?;
    inspect_execution(state, execution)
}

pub(crate) fn read_execution_input(
    state: &SessionState,
    execution_id: &str,
    offset: usize,
    limit: usize,
) -> Result<PayloadSlice, String> {
    let execution = state
        .executions
        .get(execution_id)
        .ok_or_else(|| format!("execution `{execution_id}` not found"))?;
    Ok(slice_payload(&execution.args_json, offset, limit))
}

pub(crate) fn read_execution_result(
    state: &SessionState,
    execution_id: &str,
    offset: usize,
    limit: usize,
) -> Result<PayloadSlice, String> {
    let execution = state
        .executions
        .get(execution_id)
        .ok_or_else(|| format!("execution `{execution_id}` not found"))?;
    if execution.result_message.is_empty() {
        return Err(format!(
            "execution `{execution_id}` result is not available"
        ));
    }
    Ok(slice_payload(&execution.result_message, offset, limit))
}

#[derive(Debug, Clone)]
struct ExecutionRow {
    order: u64,
    summary: ExecutionSummary,
}

#[derive(Debug, Clone, Copy)]
enum CursorBoundary {
    Before(u64),
    After(u64),
}

fn summarize_execution(
    state: &SessionState,
    execution: &pb::Execution,
) -> Option<ExecutionSummary> {
    Some(ExecutionSummary {
        execution_id: execution.execution_id.clone(),
        action_id: execution.action_id.clone(),
        state: inspect_execution(state, execution)?.state,
    })
}

fn inspect_execution(
    state: &SessionState,
    execution: &pb::Execution,
) -> Option<ExecutionInspection> {
    Some(ExecutionInspection {
        execution_id: execution.execution_id.clone(),
        action_id: execution.action_id.clone(),
        state: derive_execution_state(state, execution)?,
        input_payload: execution.args_json.clone(),
        result_payload: (!execution.result_message.is_empty())
            .then(|| execution.result_message.clone()),
        execution_time_ms: parse_execution_time_ms(&execution.result_message),
    })
}

fn derive_execution_state(
    state: &SessionState,
    execution: &pb::Execution,
) -> Option<ExecutionInspectionState> {
    match pb::ExecutionStatus::try_from(execution.status).ok()? {
        pb::ExecutionStatus::Pending | pb::ExecutionStatus::Running => {
            let execution_runtime = state.execution_runtimes.get(&execution.execution_id)?;
            let submission = state
                .execution_submissions
                .get(&execution_runtime.submission_id)?;
            match submission.status {
                ExecutionSubmissionStatus::Queued => Some(ExecutionInspectionState::Queued),
                ExecutionSubmissionStatus::RunningForeground => {
                    Some(ExecutionInspectionState::RunningForeground)
                }
                ExecutionSubmissionStatus::RunningBackground => {
                    Some(ExecutionInspectionState::RunningBackground)
                }
            }
        }
        pb::ExecutionStatus::Succeeded => Some(ExecutionInspectionState::Succeeded),
        pb::ExecutionStatus::Failed => Some(ExecutionInspectionState::Failed),
        pb::ExecutionStatus::Canceled => Some(ExecutionInspectionState::Canceled),
        pb::ExecutionStatus::Unspecified => None,
    }
}

fn parse_execution_time_ms(result_message: &str) -> Option<u64> {
    let envelope: Value = serde_json::from_str(result_message).ok()?;
    envelope.get("execution_time_ms")?.as_u64()
}

fn execution_order(execution: &pb::Execution) -> u64 {
    execution
        .execution_id
        .strip_prefix("execution-")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
}

fn normalize_execution_list_limit(limit: usize) -> usize {
    let effective = if limit == 0 {
        DEFAULT_EXECUTION_LIST_LIMIT
    } else {
        limit
    };
    effective.min(MAX_EXECUTION_LIST_LIMIT)
}

fn normalize_payload_limit(limit: usize) -> usize {
    limit.min(MAX_EXECUTION_PAYLOAD_LIMIT)
}

fn slice_payload(payload: &str, offset: usize, limit: usize) -> PayloadSlice {
    let total_size = payload.len();
    let offset = offset.min(total_size);
    let limit = normalize_payload_limit(limit);
    let end = offset.saturating_add(limit).min(total_size);
    let content = payload
        .as_bytes()
        .get(offset..end)
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default();
    PayloadSlice {
        total_size,
        offset,
        limit,
        content,
    }
}

fn encode_cursor(boundary: CursorBoundary) -> String {
    let raw = match boundary {
        CursorBoundary::Before(order) => format!("before:{order}"),
        CursorBoundary::After(order) => format!("after:{order}"),
    };
    hex_encode(raw.as_bytes())
}

fn decode_cursor(cursor: &str) -> Result<CursorBoundary, String> {
    let raw_bytes = hex_decode(cursor).ok_or_else(|| "invalid cursor".to_string())?;
    let raw = String::from_utf8(raw_bytes).map_err(|_| "invalid cursor".to_string())?;
    let (direction, order) = raw
        .split_once(':')
        .ok_or_else(|| "invalid cursor".to_string())?;
    let order = order
        .parse::<u64>()
        .map_err(|_| "invalid cursor".to_string())?;
    match direction {
        "before" => Ok(CursorBoundary::Before(order)),
        "after" => Ok(CursorBoundary::After(order)),
        _ => Err("invalid cursor".to_string()),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.as_bytes().chunks_exact(2);
    for chunk in &mut chars {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Some(bytes)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::{
        ExecutionInspectionState, ExecutionListQuery, get_execution, list_executions,
        read_execution_input, read_execution_result,
    };
    use crate::agent::SessionCompaction;
    use crate::session::state::{
        ExecutionRuntimeState, ExecutionSubmissionExecution, ExecutionSubmissionState,
        ExecutionSubmissionStatus, SessionState,
    };
    use crate::util::{default_agent_profile, default_user_profile};
    use fathom_protocol::pb;

    fn test_state() -> SessionState {
        let user_id = "user-a".to_string();
        let mut state = SessionState {
            session_id: "session-1".to_string(),
            created_at_unix_ms: 0,
            agent_id: "agent-a".to_string(),
            participant_user_ids: vec![user_id.clone()],
            agent_profile_copy: default_agent_profile("agent-a"),
            participant_user_profiles_copy: HashMap::from([(
                user_id.clone(),
                default_user_profile(&user_id),
            )]),
            trigger_queue: Default::default(),
            history: Vec::new(),
            executions: HashMap::new(),
            engaged_capability_domain_ids: BTreeSet::new(),
            foreground_submission_ids: Default::default(),
            execution_runtimes: Default::default(),
            execution_submissions: Default::default(),
            active_submission_ids_by_domain: Default::default(),
            queued_submission_ids_by_domain: Default::default(),
            pending_payload_lookups: Vec::new(),
            next_agent_invocation_seq: 0,
            turn_seq: 0,
            turn_in_progress: false,
            compaction: SessionCompaction::default(),
        };
        state.executions.insert(
            "execution-1".to_string(),
            pb::Execution {
                execution_id: "execution-1".to_string(),
                session_id: "session-1".to_string(),
                action_id: "filesystem__list".to_string(),
                args_json: "{\"path\":\".\"}".to_string(),
                status: pb::ExecutionStatus::Succeeded as i32,
                result_message: serde_json::json!({
                    "ok": true,
                    "data": {"entries": []},
                    "execution_time_ms": 12
                })
                .to_string(),
                created_at_unix_ms: 0,
                updated_at_unix_ms: 0,
            },
        );
        state.executions.insert(
            "execution-2".to_string(),
            pb::Execution {
                execution_id: "execution-2".to_string(),
                session_id: "session-1".to_string(),
                action_id: "shell__run".to_string(),
                args_json: "{\"command\":\"pwd\"}".to_string(),
                status: pb::ExecutionStatus::Running as i32,
                result_message: String::new(),
                created_at_unix_ms: 0,
                updated_at_unix_ms: 0,
            },
        );
        state.execution_runtimes.insert(
            "execution-2".to_string(),
            ExecutionRuntimeState {
                submission_id: "execution-submission-1".to_string(),
                background_requested: true,
                call_key: "call-key".to_string(),
                call_id: None,
            },
        );
        state.execution_submissions.insert(
            "execution-submission-1".to_string(),
            ExecutionSubmissionState {
                capability_domain_id: "shell".to_string(),
                executions: vec![ExecutionSubmissionExecution {
                    execution_id: "execution-2".to_string(),
                    action_key: fathom_capability_domain::CapabilityActionKey(0),
                }],
                status: ExecutionSubmissionStatus::RunningBackground,
                foreground_wait_deadline: None,
            },
        );
        state
    }

    #[test]
    fn list_executions_orders_descending_and_filters() {
        let state = test_state();
        let page = list_executions(
            &state,
            &ExecutionListQuery {
                cursor: None,
                limit: 20,
                state: Some(ExecutionInspectionState::RunningBackground),
                action_id: None,
            },
        )
        .expect("list executions");

        assert_eq!(page.executions.len(), 1);
        assert_eq!(page.executions[0].execution_id, "execution-2");
        assert!(page.next_cursor.is_none());
        assert!(page.prev_cursor.is_none());
    }

    #[test]
    fn get_execution_extracts_execution_time() {
        let state = test_state();
        let execution = get_execution(&state, "execution-1").expect("execution");

        assert_eq!(execution.action_id, "filesystem__list");
        assert_eq!(execution.execution_time_ms, Some(12));
        assert!(execution.result_payload.is_some());
    }

    #[test]
    fn read_execution_payloads_clamp_ranges() {
        let state = test_state();
        let input = read_execution_input(&state, "execution-1", 128, 1024).expect("input slice");
        assert_eq!(input.offset, "{\"path\":\".\"}".len());
        assert!(input.content.is_empty());

        let result = read_execution_result(&state, "execution-1", 0, 4).expect("result slice");
        assert_eq!(result.limit, 4);
        assert_eq!(result.content.len(), 4);
    }
}
