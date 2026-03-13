use std::collections::{BTreeSet, HashMap};

use tonic::Status;

use super::Runtime;
use crate::capability_domain::CapabilityDomainRegistry;
use crate::session::SessionState;
use crate::util::dedup_ids;
use fathom_protocol::pb;

pub(crate) struct SessionSetupRequest {
    pub(crate) agent_id: String,
    pub(crate) participant_user_ids: Vec<String>,
}

pub(crate) struct SessionSetupResolved {
    pub(crate) session_id: String,
    pub(crate) agent_id: String,
    pub(crate) participant_user_ids: Vec<String>,
    pub(crate) agent_profile_copy: pb::AgentProfile,
    pub(crate) participant_user_profiles_copy: HashMap<String, pb::UserProfile>,
    pub(crate) engaged_capability_domain_ids: BTreeSet<String>,
}

#[tonic::async_trait]
pub(crate) trait SessionSetupContext: Send + Sync {
    async fn get_or_create_agent_profile(&self, agent_id: &str) -> pb::AgentProfile;
    async fn get_or_create_user_profile(&self, user_id: &str) -> pb::UserProfile;
    fn next_session_id(&self) -> String;
}

pub(crate) struct RuntimeSessionSetupContext<'a> {
    runtime: &'a Runtime,
}

impl<'a> RuntimeSessionSetupContext<'a> {
    pub(crate) fn new(runtime: &'a Runtime) -> Self {
        Self { runtime }
    }
}

#[tonic::async_trait]
impl SessionSetupContext for RuntimeSessionSetupContext<'_> {
    async fn get_or_create_agent_profile(&self, agent_id: &str) -> pb::AgentProfile {
        self.runtime.get_or_create_agent_profile(agent_id).await
    }

    async fn get_or_create_user_profile(&self, user_id: &str) -> pb::UserProfile {
        self.runtime.get_or_create_user_profile(user_id).await
    }

    fn next_session_id(&self) -> String {
        self.runtime.next_session_id()
    }
}

#[derive(Clone)]
pub(crate) struct DefaultSessionSetupPolicy {
    registry: CapabilityDomainRegistry,
}

impl DefaultSessionSetupPolicy {
    pub(crate) fn new(registry: CapabilityDomainRegistry) -> Self {
        Self { registry }
    }
}

#[tonic::async_trait]
pub(crate) trait SessionSetupPolicy: Send + Sync {
    async fn resolve(
        &self,
        context: &dyn SessionSetupContext,
        request: SessionSetupRequest,
    ) -> Result<SessionSetupResolved, Status>;
}

#[tonic::async_trait]
impl SessionSetupPolicy for DefaultSessionSetupPolicy {
    async fn resolve(
        &self,
        context: &dyn SessionSetupContext,
        request: SessionSetupRequest,
    ) -> Result<SessionSetupResolved, Status> {
        if request.agent_id.trim().is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let participant_user_ids = dedup_ids(request.participant_user_ids);
        let agent_profile_copy = context.get_or_create_agent_profile(&request.agent_id).await;
        let mut participant_user_profiles_copy = HashMap::new();
        for user_id in &participant_user_ids {
            let profile = context.get_or_create_user_profile(user_id).await;
            participant_user_profiles_copy.insert(user_id.clone(), profile);
        }

        let engaged_capability_domain_ids = self
            .registry
            .installed_capability_domain_ids()
            .into_iter()
            .collect::<BTreeSet<_>>();

        Ok(SessionSetupResolved {
            session_id: context.next_session_id(),
            agent_id: request.agent_id,
            participant_user_ids,
            agent_profile_copy,
            participant_user_profiles_copy,
            engaged_capability_domain_ids,
        })
    }
}

pub(crate) fn build_session_state(setup: SessionSetupResolved) -> SessionState {
    SessionState::new(
        setup.session_id,
        setup.agent_id,
        setup.participant_user_ids,
        setup.agent_profile_copy,
        setup.participant_user_profiles_copy,
        setup.engaged_capability_domain_ids,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{
        DefaultSessionSetupPolicy, SessionSetupContext, SessionSetupPolicy, SessionSetupRequest,
    };
    use crate::capability_domain::build_default_capability_domain_registry;
    use crate::util::{default_agent_profile, default_user_profile};
    use fathom_protocol::pb;

    struct FakeSetupContext {
        workspace_root: PathBuf,
        agent_profiles: HashMap<String, pb::AgentProfile>,
        user_profiles: HashMap<String, pb::UserProfile>,
        next_session_id: String,
    }

    #[tonic::async_trait]
    impl SessionSetupContext for FakeSetupContext {
        async fn get_or_create_agent_profile(&self, agent_id: &str) -> pb::AgentProfile {
            self.agent_profiles
                .get(agent_id)
                .cloned()
                .unwrap_or_else(|| default_agent_profile(agent_id))
        }

        async fn get_or_create_user_profile(&self, user_id: &str) -> pb::UserProfile {
            self.user_profiles
                .get(user_id)
                .cloned()
                .unwrap_or_else(|| default_user_profile(user_id))
        }

        fn next_session_id(&self) -> String {
            self.next_session_id.clone()
        }
    }

    #[tokio::test]
    async fn default_session_setup_policy_preserves_current_defaults() {
        let context = FakeSetupContext {
            workspace_root: PathBuf::from("/tmp/fathom"),
            agent_profiles: HashMap::new(),
            user_profiles: HashMap::new(),
            next_session_id: "session-42".to_string(),
        };
        let policy = DefaultSessionSetupPolicy::new(build_default_capability_domain_registry(
            context.workspace_root.as_path(),
        ));

        let resolved = policy
            .resolve(
                &context,
                SessionSetupRequest {
                    agent_id: "agent-a".to_string(),
                    participant_user_ids: vec![
                        "user-a".to_string(),
                        "user-b".to_string(),
                        "user-a".to_string(),
                    ],
                },
            )
            .await
            .expect("setup should resolve");

        assert_eq!(resolved.session_id, "session-42");
        assert_eq!(
            resolved.participant_user_ids,
            vec!["user-a".to_string(), "user-b".to_string()]
        );
        assert!(
            resolved
                .engaged_capability_domain_ids
                .contains(fathom_capability_domain_fs::FILESYSTEM_CAPABILITY_DOMAIN_ID)
        );
        assert!(
            resolved
                .engaged_capability_domain_ids
                .contains(fathom_capability_domain_shell::SHELL_CAPABILITY_DOMAIN_ID)
        );
    }
}
