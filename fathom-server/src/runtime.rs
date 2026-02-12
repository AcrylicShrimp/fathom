use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, bail};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};
use tonic::Status;

use crate::agent::{AgentOrchestrator, TurnSnapshot};
use crate::pb;
use crate::session::{SessionCommand, SessionRuntime, SessionState, run_session_actor};
use crate::util::{dedup_ids, default_agent_profile, default_user_profile, now_unix_ms};

pub(crate) const EVENT_BUFFER_SIZE: usize = 256;
pub(crate) const SESSION_CMD_BUFFER_SIZE: usize = 128;
pub(crate) const DEFAULT_TASK_CAPACITY: usize = 4;
pub(crate) const DEFAULT_TASK_RUNTIME_MS: u64 = 500;

#[derive(Clone)]
pub(crate) struct Runtime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    sessions: RwLock<HashMap<String, SessionRuntime>>,
    user_profiles: RwLock<HashMap<String, pb::UserProfile>>,
    agent_profiles: RwLock<HashMap<String, pb::AgentProfile>>,
    workspace_root: PathBuf,
    session_seq: AtomicU64,
    trigger_seq: AtomicU64,
    task_seq: AtomicU64,
    task_capacity: usize,
    task_runtime_ms: u64,
    orchestrator: AgentOrchestrator,
}

impl Runtime {
    pub(crate) fn new(task_capacity: usize, task_runtime_ms: u64) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new_with_workspace_root(task_capacity, task_runtime_ms, workspace_root)
            .unwrap_or_else(|_| {
                Self::new_unchecked(task_capacity, task_runtime_ms, PathBuf::from("."))
            })
    }

    pub(crate) fn new_with_workspace_root(
        task_capacity: usize,
        task_runtime_ms: u64,
        workspace_root: PathBuf,
    ) -> anyhow::Result<Self> {
        let workspace_root = canonicalize_workspace_root(workspace_root)?;
        Ok(Self::new_unchecked(
            task_capacity,
            task_runtime_ms,
            workspace_root,
        ))
    }

    fn new_unchecked(task_capacity: usize, task_runtime_ms: u64, workspace_root: PathBuf) -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                sessions: RwLock::new(HashMap::new()),
                user_profiles: RwLock::new(HashMap::new()),
                agent_profiles: RwLock::new(HashMap::new()),
                workspace_root,
                session_seq: AtomicU64::new(0),
                trigger_seq: AtomicU64::new(0),
                task_seq: AtomicU64::new(0),
                task_capacity,
                task_runtime_ms,
                orchestrator: AgentOrchestrator::new(),
            }),
        }
    }

    fn next_session_id(&self) -> String {
        format!(
            "session-{}",
            self.inner.session_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub(crate) fn next_trigger_id(&self) -> String {
        format!(
            "trigger-{}",
            self.inner.trigger_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub(crate) fn next_task_id(&self) -> String {
        format!(
            "task-{}",
            self.inner.task_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub(crate) fn task_capacity(&self) -> usize {
        self.inner.task_capacity
    }

    pub(crate) fn task_runtime_ms(&self) -> u64 {
        self.inner.task_runtime_ms
    }

    pub(crate) fn workspace_root(&self) -> &Path {
        self.inner.workspace_root.as_path()
    }

    pub(crate) fn agent_orchestrator(&self) -> AgentOrchestrator {
        self.inner.orchestrator.clone()
    }

    pub(crate) fn build_turn_snapshot(
        &self,
        state: &SessionState,
        turn_id: u64,
        triggers: &[pb::Trigger],
    ) -> TurnSnapshot {
        const HISTORY_WINDOW_SIZE: usize = 80;
        let recent_history = if state.history.len() > HISTORY_WINDOW_SIZE {
            state.history[state.history.len() - HISTORY_WINDOW_SIZE..].to_vec()
        } else {
            state.history.clone()
        };

        let participant_profiles = state
            .participant_user_ids
            .iter()
            .filter_map(|id| state.participant_user_profiles_copy.get(id).cloned())
            .collect::<Vec<_>>();

        TurnSnapshot {
            session_id: state.session_id.clone(),
            turn_id,
            agent_profile: state.agent_profile_copy.clone(),
            participant_profiles,
            triggers: triggers.to_vec(),
            recent_history,
            compaction: state.compaction.clone(),
        }
    }

    pub(crate) async fn create_session(
        &self,
        agent_id: String,
        participant_user_ids: Vec<String>,
    ) -> Result<pb::SessionSummary, Status> {
        if agent_id.trim().is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let participant_user_ids = dedup_ids(participant_user_ids);
        let agent_profile_copy = self.get_or_create_agent_profile(&agent_id).await;
        let mut participant_user_profiles_copy = HashMap::new();

        for user_id in &participant_user_ids {
            let profile = self.get_or_create_user_profile(user_id).await;
            participant_user_profiles_copy.insert(user_id.clone(), profile);
        }

        let session_id = self.next_session_id();
        let state = SessionState::new(
            session_id.clone(),
            agent_id,
            participant_user_ids,
            agent_profile_copy,
            participant_user_profiles_copy,
        );
        let session_summary = state.to_summary();

        let (events_tx, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        let (command_tx, command_rx) = mpsc::channel(SESSION_CMD_BUFFER_SIZE);

        tokio::spawn(run_session_actor(
            self.clone(),
            state,
            command_tx.clone(),
            command_rx,
            events_tx.clone(),
        ));

        self.inner.sessions.write().await.insert(
            session_id,
            SessionRuntime {
                command_tx,
                events_tx,
            },
        );

        Ok(session_summary)
    }

    pub(crate) async fn list_sessions(&self) -> Result<Vec<pb::SessionSummary>, Status> {
        let sessions = self
            .inner
            .sessions
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let mut summaries = Vec::with_capacity(sessions.len());
        for session in sessions {
            let (response_tx, response_rx) = oneshot::channel();
            session
                .command_tx
                .send(SessionCommand::GetSummary {
                    respond_to: response_tx,
                })
                .await
                .map_err(|_| Status::unavailable("session actor unavailable"))?;
            let summary = response_rx
                .await
                .map_err(|_| Status::unavailable("session summary unavailable"))?;
            summaries.push(summary);
        }

        summaries.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(summaries)
    }

    pub(crate) async fn get_session(&self, session_id: &str) -> Result<SessionRuntime, Status> {
        self.inner
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| Status::not_found("session not found"))
    }

    pub(crate) async fn enqueue_trigger(
        &self,
        session_id: &str,
        trigger: pb::Trigger,
    ) -> Result<pb::EnqueueTriggerResponse, Status> {
        let session = self.get_session(session_id).await?;
        let (response_tx, response_rx) = oneshot::channel();
        session
            .command_tx
            .send(SessionCommand::EnqueueTrigger {
                trigger,
                respond_to: response_tx,
            })
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?;
        response_rx
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?
    }

    pub(crate) async fn list_tasks(&self, session_id: &str) -> Result<Vec<pb::Task>, Status> {
        let session = self.get_session(session_id).await?;
        let (response_tx, response_rx) = oneshot::channel();
        session
            .command_tx
            .send(SessionCommand::ListTasks {
                respond_to: response_tx,
            })
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?;
        response_rx
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))
    }

    pub(crate) async fn cancel_task(
        &self,
        session_id: &str,
        task_id: String,
    ) -> Result<pb::CancelTaskResponse, Status> {
        let session = self.get_session(session_id).await?;
        let (response_tx, response_rx) = oneshot::channel();
        session
            .command_tx
            .send(SessionCommand::CancelTask {
                task_id,
                respond_to: response_tx,
            })
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?;
        response_rx
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?
    }

    pub(crate) async fn get_or_create_user_profile(&self, user_id: &str) -> pb::UserProfile {
        let mut profiles = self.inner.user_profiles.write().await;
        let profile = profiles
            .entry(user_id.to_string())
            .or_insert_with(|| default_user_profile(user_id));
        profile.clone()
    }

    pub(crate) async fn get_or_create_agent_profile(&self, agent_id: &str) -> pb::AgentProfile {
        let mut profiles = self.inner.agent_profiles.write().await;
        let profile = profiles
            .entry(agent_id.to_string())
            .or_insert_with(|| default_agent_profile(agent_id));
        profile.clone()
    }

    pub(crate) async fn upsert_user_profile(
        &self,
        mut profile: pb::UserProfile,
    ) -> Result<pb::UserProfile, Status> {
        if profile.user_id.trim().is_empty() {
            return Err(Status::invalid_argument("profile.user_id is required"));
        }
        if profile.updated_at_unix_ms == 0 {
            profile.updated_at_unix_ms = now_unix_ms();
        }

        self.inner
            .user_profiles
            .write()
            .await
            .insert(profile.user_id.clone(), profile.clone());
        Ok(profile)
    }

    pub(crate) async fn upsert_agent_profile(
        &self,
        mut profile: pb::AgentProfile,
    ) -> Result<pb::AgentProfile, Status> {
        if profile.agent_id.trim().is_empty() {
            return Err(Status::invalid_argument("profile.agent_id is required"));
        }

        let mut profiles = self.inner.agent_profiles.write().await;
        let current_version = profiles
            .get(&profile.agent_id)
            .map(|current| current.spec_version)
            .unwrap_or(0);
        if profile.spec_version == 0 {
            profile.spec_version = current_version.max(1) + 1;
        }
        if profile.updated_at_unix_ms == 0 {
            profile.updated_at_unix_ms = now_unix_ms();
        }

        profiles.insert(profile.agent_id.clone(), profile.clone());
        Ok(profile)
    }

    pub(crate) async fn fetch_agent_profile(&self, agent_id: &str) -> Option<pb::AgentProfile> {
        self.inner
            .agent_profiles
            .read()
            .await
            .get(agent_id)
            .cloned()
    }

    pub(crate) async fn fetch_user_profile(&self, user_id: &str) -> Option<pb::UserProfile> {
        self.inner.user_profiles.read().await.get(user_id).cloned()
    }
}

fn canonicalize_workspace_root(workspace_root: PathBuf) -> anyhow::Result<PathBuf> {
    let workspace_root = if workspace_root.is_absolute() {
        workspace_root
    } else {
        std::env::current_dir()
            .context("failed to resolve current working directory")?
            .join(workspace_root)
    };

    let canonical = std::fs::canonicalize(&workspace_root).with_context(|| {
        format!(
            "failed to resolve workspace root `{}`",
            workspace_root.display()
        )
    })?;
    let metadata = std::fs::metadata(&canonical).with_context(|| {
        format!(
            "failed to read workspace root metadata `{}`",
            canonical.display()
        )
    })?;
    if !metadata.is_dir() {
        bail!(
            "workspace root must be a directory: `{}`",
            canonical.display()
        );
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::Runtime;

    #[tokio::test]
    async fn creates_session_with_profile_copies() {
        let runtime = Runtime::new(2, 10);
        let session = runtime
            .create_session("agent-a".to_string(), vec!["user-a".to_string()])
            .await
            .expect("create session");

        assert_eq!(session.agent_id, "agent-a");
        assert_eq!(session.participant_user_ids, vec!["user-a".to_string()]);
        assert!(session.agent_profile_copy.is_some());
        assert_eq!(session.participant_user_profiles_copy.len(), 1);
    }
}
