use std::collections::BTreeSet;

use serde_json::Value;

use crate::environment::EnvironmentRegistry;

use super::types::AgentInvocationContext;

#[derive(Clone)]
pub(crate) struct SessionActionCatalog {
    registry: EnvironmentRegistry,
    engaged_environment_ids: BTreeSet<String>,
}

impl SessionActionCatalog {
    pub(crate) fn from_context(
        registry: EnvironmentRegistry,
        context: &AgentInvocationContext,
    ) -> Self {
        Self {
            registry,
            engaged_environment_ids: context
                .session_baseline
                .capability_surface
                .environments
                .iter()
                .map(|environment| environment.id.clone())
                .collect(),
        }
    }

    pub(crate) fn openai_action_definitions(&self) -> Vec<Value> {
        self.registry
            .openai_action_definitions_for_environments(&self.engaged_environment_ids)
    }

    pub(crate) fn validate_action(&self, action_id: &str, args: &Value) -> Result<String, String> {
        self.registry
            .validate_in_environments(action_id, args, &self.engaged_environment_ids)
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::SessionActionCatalog;
    use crate::agent::types::{
        AgentInvocationContext, CapabilityAction, CapabilityEnvironment, CapabilitySurface,
        HarnessContract, IdentityEnvelope, ParticipantEnvelope, SessionAnchor, SessionBaseline,
        SessionCompaction,
    };
    use crate::environment::EnvironmentRegistry;
    use serde_json::json;

    fn context_with_environments(
        environments: Vec<CapabilityEnvironment>,
    ) -> AgentInvocationContext {
        AgentInvocationContext {
            harness_contract: HarnessContract {
                runtime_version: "0.1.0".to_string(),
                contract_schema_version: 1,
            },
            identity_envelope: IdentityEnvelope {
                schema_version: 1,
                source_revision: "agent-default@spec:1@updated:1".to_string(),
                material: json!({"display_name": "Agent Default"}),
            },
            session_baseline: SessionBaseline {
                session_anchor: SessionAnchor {
                    session_id: "session-1".to_string(),
                    started_at_unix_ms: 1,
                },
                capability_surface: CapabilitySurface { environments },
                participant_envelope: ParticipantEnvelope {
                    schema_version: 1,
                    source_revision: "user-default@1".to_string(),
                    material: json!({"participants": []}),
                },
            },
            resolved_payload_lookups: vec![],
            triggers: vec![],
            recent_history: vec![],
            compaction: SessionCompaction::default(),
        }
    }

    #[test]
    fn action_catalog_limits_openai_definitions_to_context_environments() {
        let context = context_with_environments(vec![CapabilityEnvironment {
            id: "filesystem".to_string(),
            name: "Filesystem".to_string(),
            description: "Filesystem".to_string(),
            actions: vec![CapabilityAction {
                action_id: "filesystem__list".to_string(),
                description: "List files".to_string(),
                mode_support: crate::agent::ActionModeSupportContract::AwaitOnly,
                discovery: false,
            }],
            recipes: vec![],
        }]);

        let catalog = SessionActionCatalog::from_context(EnvironmentRegistry::new(), &context);
        let definitions = catalog.openai_action_definitions();
        let names = definitions
            .iter()
            .filter_map(|item| item.get("name").and_then(|name| name.as_str()))
            .collect::<Vec<_>>();

        assert!(names.contains(&"filesystem__list"));
        assert!(!names.contains(&"shell__run"));
        assert!(!names.contains(&"system__get_time"));
    }

    #[test]
    fn action_catalog_rejects_actions_outside_context_environments() {
        let context = context_with_environments(vec![CapabilityEnvironment {
            id: "filesystem".to_string(),
            name: "Filesystem".to_string(),
            description: "Filesystem".to_string(),
            actions: vec![CapabilityAction {
                action_id: "filesystem__list".to_string(),
                description: "List files".to_string(),
                mode_support: crate::agent::ActionModeSupportContract::AwaitOnly,
                discovery: false,
            }],
            recipes: vec![],
        }]);

        let catalog = SessionActionCatalog::from_context(EnvironmentRegistry::new(), &context);
        let error = catalog
            .validate_action(
                "shell__run",
                &json!({"command": "pwd", "reasoning": "inspect cwd"}),
            )
            .expect_err("shell action should be rejected");

        assert!(error.contains("is not available in this session"));
    }
}
