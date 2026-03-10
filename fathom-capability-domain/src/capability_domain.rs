use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Action;

#[derive(Debug, Clone)]
pub struct CapabilityDomainSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDomainRecipe {
    pub title: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDomainSnapshot {
    pub capability_domain_id: String,
    pub schema_version: u32,
    pub state_json: Value,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone)]
pub struct FinalizedAction {
    pub seq: u64,
    pub canonical_action_id: String,
    pub action_name: String,
    pub args_json: String,
    pub succeeded: bool,
    pub message: String,
    pub state_patch: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct TransitionResult {
    pub state_patch: Option<Value>,
    pub transition_events: Vec<Value>,
}

impl TransitionResult {
    pub fn no_change() -> Self {
        Self {
            state_patch: None,
            transition_events: Vec::new(),
        }
    }
}

pub trait CapabilityDomain: Send + Sync + 'static {
    fn spec(&self) -> CapabilityDomainSpec;

    fn schema_version(&self) -> u32 {
        1
    }

    fn initial_state(&self) -> Value;

    fn actions(&self) -> Vec<Arc<dyn Action>>;

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        Vec::new()
    }

    fn apply_transition(
        &self,
        _current_state: &Value,
        finalized: &FinalizedAction,
    ) -> Result<TransitionResult, String> {
        Ok(TransitionResult {
            state_patch: finalized.state_patch.clone(),
            transition_events: Vec::new(),
        })
    }
}
