use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub succeeded: bool,
    pub message: String,
    pub state_patch: Option<Value>,
}
