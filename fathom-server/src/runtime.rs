mod context_snapshot;
mod diagnostics;
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
use diagnostics::DiagnosticsSink;

pub(crate) const EVENT_BUFFER_SIZE: usize = 256;
pub(crate) const SESSION_CMD_BUFFER_SIZE: usize = 128;
pub(crate) const DEFAULT_EXECUTION_CAPACITY: usize = 4;

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
    execution_seq: AtomicU64,
    execution_capacity: usize,
    orchestrator: AgentOrchestrator,
    diagnostics: DiagnosticsSink,
}

impl Runtime {
    pub(crate) fn new(execution_capacity: usize, _execution_runtime_ms: u64) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new_with_workspace_root(execution_capacity, _execution_runtime_ms, workspace_root)
            .unwrap_or_else(|_| {
                Self::new_unchecked(
                    execution_capacity,
                    _execution_runtime_ms,
                    PathBuf::from("."),
                )
            })
    }

    pub(crate) fn new_with_workspace_root(
        execution_capacity: usize,
        _execution_runtime_ms: u64,
        workspace_root: PathBuf,
    ) -> anyhow::Result<Self> {
        let workspace_root = workspace::canonicalize_workspace_root(workspace_root)?;
        Ok(Self::new_unchecked(
            execution_capacity,
            _execution_runtime_ms,
            workspace_root,
        ))
    }

    fn new_unchecked(
        execution_capacity: usize,
        _execution_runtime_ms: u64,
        workspace_root: PathBuf,
    ) -> Self {
        let diagnostics = DiagnosticsSink::new(workspace_root.join(".fathom").join("diagnostics"));
        Self {
            inner: Arc::new(RuntimeInner {
                sessions: RwLock::new(HashMap::new()),
                user_profiles: RwLock::new(HashMap::new()),
                agent_profiles: RwLock::new(HashMap::new()),
                workspace_root,
                session_seq: AtomicU64::new(0),
                trigger_seq: AtomicU64::new(0),
                execution_seq: AtomicU64::new(0),
                execution_capacity,
                orchestrator: AgentOrchestrator::new(),
                diagnostics,
            }),
        }
    }

    pub(crate) fn execution_capacity(&self) -> usize {
        self.inner.execution_capacity
    }

    pub(crate) fn workspace_root(&self) -> &Path {
        self.inner.workspace_root.as_path()
    }

    pub(crate) fn agent_orchestrator(&self) -> AgentOrchestrator {
        self.inner.orchestrator.clone()
    }

    pub(crate) fn diagnostics(&self) -> DiagnosticsSink {
        self.inner.diagnostics.clone()
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
