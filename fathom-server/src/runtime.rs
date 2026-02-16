mod context_snapshot;
mod ids;
mod profiles;
mod sessions;
mod time_context;
mod workspace;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::RwLock;

use crate::agent::AgentOrchestrator;
use crate::pb;
use crate::session::SessionRuntime;

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
        let workspace_root = workspace::canonicalize_workspace_root(workspace_root)?;
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
