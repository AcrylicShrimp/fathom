use std::collections::{BTreeSet, HashMap, VecDeque};

use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::Status;

use crate::agent::SessionCompactionSnapshot;
use crate::environment::{EnvironmentCommittedAction, RequestedExecutionMode};
use crate::history::HistoryEvent;
use crate::session::payload_lookup::ResolvedPayloadLookup;
use crate::util::now_unix_ms;
use fathom_env::EnvironmentSnapshot;
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
    CancelExecution {
        execution_id: String,
        respond_to: oneshot::Sender<Result<pb::CancelExecutionResponse, Status>>,
    },
    EnvironmentActionCommitted {
        committed: EnvironmentCommittedAction,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct InFlightActionState {
    pub(crate) execution_id: String,
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) env_seq: u64,
    pub(crate) status: String,
    pub(crate) submitted_at_unix_ms: i64,
    pub(crate) args_preview: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveExecutionState {
    pub(crate) requested_mode: RequestedExecutionMode,
    pub(crate) call_key: String,
    pub(crate) call_id: Option<String>,
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
    pub(crate) engaged_environment_ids: BTreeSet<String>,
    pub(crate) environment_snapshots: HashMap<String, EnvironmentSnapshot>,
    pub(crate) next_environment_seq: HashMap<String, u64>,
    pub(crate) in_flight_actions: HashMap<String, InFlightActionState>,
    pub(crate) active_executions: HashMap<String, ActiveExecutionState>,
    pub(crate) pending_payload_lookups: Vec<ResolvedPayloadLookup>,
    pub(crate) next_agent_invocation_seq: u64,
    pub(crate) turn_seq: u64,
    pub(crate) turn_in_progress: bool,
    pub(crate) compaction: SessionCompactionSnapshot,
}

impl SessionState {
    pub(crate) fn new(
        session_id: String,
        agent_id: String,
        participant_user_ids: Vec<String>,
        agent_profile_copy: pb::AgentProfile,
        participant_user_profiles_copy: HashMap<String, pb::UserProfile>,
        engaged_environment_ids: BTreeSet<String>,
        environment_snapshots: HashMap<String, EnvironmentSnapshot>,
    ) -> Self {
        let next_environment_seq = engaged_environment_ids
            .iter()
            .map(|env_id| (env_id.clone(), 0u64))
            .collect::<HashMap<_, _>>();

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
            engaged_environment_ids,
            environment_snapshots,
            next_environment_seq,
            in_flight_actions: HashMap::new(),
            active_executions: HashMap::new(),
            pending_payload_lookups: Vec::new(),
            next_agent_invocation_seq: 0,
            turn_seq: 0,
            turn_in_progress: false,
            compaction: SessionCompactionSnapshot::default(),
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

    pub(crate) fn allocate_environment_seq(&mut self, environment_id: &str) -> u64 {
        let seq = self
            .next_environment_seq
            .entry(environment_id.to_string())
            .or_insert(0);
        *seq += 1;
        *seq
    }

    pub(crate) fn push_pending_payload_lookup(&mut self, lookup: ResolvedPayloadLookup) {
        if self
            .pending_payload_lookups
            .iter()
            .any(|item| item.lookup_execution_id == lookup.lookup_execution_id)
        {
            return;
        }
        self.pending_payload_lookups.push(lookup);
    }

    pub(crate) fn allocate_agent_invocation_seq(&mut self) -> u64 {
        self.next_agent_invocation_seq += 1;
        self.next_agent_invocation_seq
    }
}
