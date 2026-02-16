use crate::ActionOutcome;
use futures_util::future::BoxFuture;
use serde_json::Value;

pub trait ActionHost: Send + Sync {
    fn execute_environment_action<'a>(
        &'a self,
        environment_id: &'a str,
        action_name: &'a str,
        args_json: &'a str,
    ) -> BoxFuture<'a, Option<ActionOutcome>>;
}

pub struct ActionCall<'a> {
    pub host: &'a dyn ActionHost,
    pub args_json: &'a str,
    pub args: &'a Value,
    pub environment_state: &'a Value,
}
