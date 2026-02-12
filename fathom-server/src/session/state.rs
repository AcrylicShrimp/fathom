use std::collections::{HashMap, HashSet, VecDeque};

use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::Status;

use crate::pb;
use crate::util::now_unix_ms;

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
    ListTasks {
        respond_to: oneshot::Sender<Vec<pb::Task>>,
    },
    CancelTask {
        task_id: String,
        respond_to: oneshot::Sender<Result<pb::CancelTaskResponse, Status>>,
    },
    TaskFinished {
        task_id: String,
        succeeded: bool,
        message: String,
    },
}

pub(crate) struct SessionState {
    pub(crate) session_id: String,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) agent_id: String,
    pub(crate) participant_user_ids: Vec<String>,
    pub(crate) agent_profile_copy: pb::AgentProfile,
    pub(crate) participant_user_profiles_copy: HashMap<String, pb::UserProfile>,
    pub(crate) trigger_queue: VecDeque<pb::Trigger>,
    pub(crate) history: Vec<String>,
    pub(crate) tasks: HashMap<String, pb::Task>,
    pub(crate) pending_task_ids: VecDeque<String>,
    pub(crate) running_task_ids: HashSet<String>,
    pub(crate) turn_seq: u64,
    pub(crate) turn_in_progress: bool,
}

impl SessionState {
    pub(crate) fn new(
        session_id: String,
        agent_id: String,
        participant_user_ids: Vec<String>,
        agent_profile_copy: pb::AgentProfile,
        participant_user_profiles_copy: HashMap<String, pb::UserProfile>,
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
            tasks: HashMap::new(),
            pending_task_ids: VecDeque::new(),
            running_task_ids: HashSet::new(),
            turn_seq: 0,
            turn_in_progress: false,
        }
    }

    pub(crate) fn to_summary(&self) -> pb::SessionSummary {
        let participant_user_profiles_copy = self
            .participant_user_ids
            .iter()
            .filter_map(|id| self.participant_user_profiles_copy.get(id).cloned())
            .collect::<Vec<_>>();

        let pending_task_count = self
            .tasks
            .values()
            .filter(|task| task.status == pb::TaskStatus::Pending as i32)
            .count() as u64;
        let running_task_count = self
            .tasks
            .values()
            .filter(|task| task.status == pb::TaskStatus::Running as i32)
            .count() as u64;

        pb::SessionSummary {
            session_id: self.session_id.clone(),
            created_at_unix_ms: self.created_at_unix_ms,
            agent_id: self.agent_id.clone(),
            participant_user_ids: self.participant_user_ids.clone(),
            agent_profile_copy: Some(self.agent_profile_copy.clone()),
            participant_user_profiles_copy,
            queued_trigger_count: self.trigger_queue.len() as u64,
            history_entry_count: self.history.len() as u64,
            pending_task_count,
            running_task_count,
        }
    }
}
