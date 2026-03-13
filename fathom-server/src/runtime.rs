mod diagnostics;
mod ids;
mod invocation_context;
mod profiles;
mod session_setup;
mod sessions;
mod system_inspection;
mod workspace;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::RwLock;

use crate::agent::AgentOrchestrator;
use crate::capability_domain::{CapabilityDomainRegistry, build_capability_domain_registry};
use crate::session::SessionRuntime;
use diagnostics::DiagnosticsSink;
use fathom_protocol::pb;
use system_inspection::RuntimeSystemInspectionService;

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
    session_seq: AtomicU64,
    trigger_seq: AtomicU64,
    execution_seq: AtomicU64,
    execution_submission_seq: AtomicU64,
    capability_domain_registry: CapabilityDomainRegistry,
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
        _execution_capacity: usize,
        _execution_runtime_ms: u64,
        workspace_root: PathBuf,
    ) -> Self {
        let diagnostics = DiagnosticsSink::new(workspace_root.join(".fathom").join("diagnostics"));
        Self {
            inner: Arc::new_cyclic(|weak_inner| {
                let capability_domain_registry = build_capability_domain_registry(
                    &workspace_root,
                    Arc::new(RuntimeSystemInspectionService::new(weak_inner.clone())),
                );
                RuntimeInner {
                    sessions: RwLock::new(HashMap::new()),
                    user_profiles: RwLock::new(HashMap::new()),
                    agent_profiles: RwLock::new(HashMap::new()),
                    session_seq: AtomicU64::new(0),
                    trigger_seq: AtomicU64::new(0),
                    execution_seq: AtomicU64::new(0),
                    execution_submission_seq: AtomicU64::new(0),
                    capability_domain_registry: capability_domain_registry.clone(),
                    orchestrator: AgentOrchestrator::new(capability_domain_registry),
                    diagnostics: diagnostics.clone(),
                }
            }),
        }
    }

    pub(crate) fn capability_domain_registry(&self) -> CapabilityDomainRegistry {
        self.inner.capability_domain_registry.clone()
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
