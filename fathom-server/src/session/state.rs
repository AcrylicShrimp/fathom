use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use tokio::time::Instant;

use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::Status;

use crate::agent::SessionCompaction;
use crate::capability_domain::CapabilityDomainCommittedAction;
use crate::history::HistoryEvent;
use crate::session::inspection::{
    ExecutionInspection, ExecutionListPage, ExecutionListQuery, PayloadSlice,
};
use crate::session::payload_lookup::ResolvedPayloadLookup;
use crate::util::now_unix_ms;
use fathom_capability_domain::CapabilityActionKey;
use fathom_protocol::pb;

#[derive(Clone)]
pub(crate) struct SessionRuntime {
    pub(crate) command_tx: mpsc::Sender<SessionCommand>,
    pub(crate) events_tx: broadcast::Sender<pb::SessionEvent>,
}

pub(crate) enum SessionCommand {
    EnqueueTrigger {
        trigger: pb::Trigger,
        respond_to: oneshot::Sender<Result<pb::EnqueueTriggerResponse, Status>>,
    },
    GetSummary {
        respond_to: oneshot::Sender<pb::SessionSummary>,
    },
    ListExecutions {
        respond_to: oneshot::Sender<Vec<pb::Execution>>,
    },
    InspectListExecutions {
        query: ExecutionListQuery,
        respond_to: oneshot::Sender<Result<ExecutionListPage, String>>,
    },
    InspectGetExecution {
        execution_id: String,
        respond_to: oneshot::Sender<Result<Option<ExecutionInspection>, String>>,
    },
    InspectReadExecutionInput {
        execution_id: String,
        offset: usize,
        limit: usize,
        respond_to: oneshot::Sender<Result<PayloadSlice, String>>,
    },
    InspectReadExecutionResult {
        execution_id: String,
        offset: usize,
        limit: usize,
        respond_to: oneshot::Sender<Result<PayloadSlice, String>>,
    },
    CancelExecution {
        execution_id: String,
        respond_to: oneshot::Sender<Result<pb::CancelExecutionResponse, Status>>,
    },
    CapabilityDomainActionCommitted {
        committed: CapabilityDomainCommittedAction,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionRuntimeState {
    pub(crate) submission_id: String,
    pub(crate) background_requested: bool,
    pub(crate) call_key: String,
    pub(crate) call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionSubmissionStatus {
    Queued,
    RunningForeground,
    RunningBackground,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionSubmissionExecution {
    pub(crate) execution_id: String,
    pub(crate) action_key: CapabilityActionKey,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionSubmissionState {
    pub(crate) capability_domain_id: String,
    pub(crate) executions: Vec<ExecutionSubmissionExecution>,
    pub(crate) status: ExecutionSubmissionStatus,
    pub(crate) foreground_wait_deadline: Option<Instant>,
}

pub(crate) struct SessionState {
    pub(crate) session_id: String,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) agent_id: String,
    pub(crate) participant_user_ids: Vec<String>,
    pub(crate) agent_profile_copy: pb::AgentProfile,
    pub(crate) participant_user_profiles_copy: HashMap<String, pb::UserProfile>,
    pub(crate) trigger_queue: VecDeque<pb::Trigger>,
    pub(crate) history: Vec<HistoryEvent>,
    pub(crate) executions: HashMap<String, pb::Execution>,
    pub(crate) engaged_capability_domain_ids: BTreeSet<String>,
    pub(crate) foreground_submission_ids: HashSet<String>,
    pub(crate) execution_runtimes: HashMap<String, ExecutionRuntimeState>,
    pub(crate) execution_submissions: HashMap<String, ExecutionSubmissionState>,
    pub(crate) active_submission_ids_by_domain: HashMap<String, String>,
    pub(crate) queued_submission_ids_by_domain: HashMap<String, VecDeque<String>>,
    pub(crate) pending_payload_lookups: Vec<ResolvedPayloadLookup>,
    pub(crate) next_agent_invocation_seq: u64,
    pub(crate) turn_seq: u64,
    pub(crate) turn_in_progress: bool,
    pub(crate) compaction: SessionCompaction,
}

impl SessionState {
    pub(crate) fn new(
        session_id: String,
        agent_id: String,
        participant_user_ids: Vec<String>,
        agent_profile_copy: pb::AgentProfile,
        participant_user_profiles_copy: HashMap<String, pb::UserProfile>,
        engaged_capability_domain_ids: BTreeSet<String>,
    ) -> Self {
        Self {
            session_id,
            created_at_unix_ms: now_unix_ms(),
            agent_id,
            participant_user_ids,
            agent_profile_copy,
            participant_user_profiles_copy,
            trigger_queue: VecDeque::new(),
            history: Vec::new(),
            executions: HashMap::new(),
            engaged_capability_domain_ids,
            foreground_submission_ids: HashSet::new(),
            execution_runtimes: HashMap::new(),
            execution_submissions: HashMap::new(),
            active_submission_ids_by_domain: HashMap::new(),
            queued_submission_ids_by_domain: HashMap::new(),
            pending_payload_lookups: Vec::new(),
            next_agent_invocation_seq: 0,
            turn_seq: 0,
            turn_in_progress: false,
            compaction: SessionCompaction::default(),
        }
    }

    pub(crate) fn to_summary(&self) -> pb::SessionSummary {
        let participant_user_profiles_copy = self
            .participant_user_ids
            .iter()
            .filter_map(|id| self.participant_user_profiles_copy.get(id).cloned())
            .collect::<Vec<_>>();

        let pending_execution_count = self
            .executions
            .values()
            .filter(|execution| execution.status == pb::ExecutionStatus::Pending as i32)
            .count() as u64;
        let running_execution_count = self
            .executions
            .values()
            .filter(|execution| execution.status == pb::ExecutionStatus::Running as i32)
            .count() as u64;

        pb::SessionSummary {
            session_id: self.session_id.clone(),
            created_at_unix_ms: self.created_at_unix_ms,
            agent_id: self.agent_id.clone(),
            participant_user_ids: self.participant_user_ids.clone(),
            agent_profile_copy: Some(self.agent_profile_copy.clone()),
            participant_user_profiles_copy,
            queued_trigger_count: self.trigger_queue.len() as u64,
            history_entry_count: self.compaction.last_compacted_history_index
                + self.history.len() as u64,
            pending_execution_count,
            running_execution_count,
        }
    }

    pub(crate) fn push_pending_payload_lookup(&mut self, lookup: ResolvedPayloadLookup) {
        if self.pending_payload_lookups.iter().any(|item| {
            item.execution_id == lookup.execution_id
                && item.part == lookup.part
                && item.offset == lookup.offset
        }) {
            return;
        }
        self.pending_payload_lookups.push(lookup);
    }

    pub(crate) fn allocate_agent_invocation_seq(&mut self) -> u64 {
        self.next_agent_invocation_seq += 1;
        self.next_agent_invocation_seq
    }

    pub(crate) fn has_blocking_submissions(&self) -> bool {
        !self.foreground_submission_ids.is_empty()
    }

    pub(crate) fn next_foreground_wait_deadline(&self) -> Option<Instant> {
        self.foreground_submission_ids
            .iter()
            .filter_map(|submission_id| {
                self.execution_submissions
                    .get(submission_id)
                    .and_then(|submission| submission.foreground_wait_deadline)
            })
            .min()
    }
}
