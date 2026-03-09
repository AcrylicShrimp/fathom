use std::collections::BTreeSet;

use serde_json::Value;

use crate::environment::EnvironmentRegistry;

use super::types::TurnSnapshot;

#[derive(Clone)]
pub(crate) struct SessionToolCatalog {
    registry: EnvironmentRegistry,
    engaged_environment_ids: BTreeSet<String>,
}

impl SessionToolCatalog {
    pub(crate) fn from_snapshot(registry: EnvironmentRegistry, snapshot: &TurnSnapshot) -> Self {
        Self {
            registry,
            engaged_environment_ids: snapshot
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
