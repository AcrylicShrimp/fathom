use crate::{ActionCall, ActionOutcome};
use futures_util::future::BoxFuture;
use serde_json::Value;

pub type ActionFuture<'a> = BoxFuture<'a, Option<ActionOutcome>>;

#[derive(Debug, Clone)]
pub struct ActionSpec {
    pub environment_id: &'static str,
    pub action_name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub discovery: bool,
}

impl ActionSpec {
    pub fn canonical_id(&self) -> String {
        format!("{}__{}", self.environment_id, self.action_name)
    }
}

pub trait Action: Send + Sync {
    fn spec(&self) -> ActionSpec;

    fn validate(&self, args: &Value) -> Result<(), String>;

    fn execute<'a>(&'a self, call: ActionCall<'a>) -> ActionFuture<'a>;
}
