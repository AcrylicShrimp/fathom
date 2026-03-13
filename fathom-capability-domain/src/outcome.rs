use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSuccess {
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInputError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRuntimeError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionError {
    InputError(ActionInputError),
    RuntimeError(ActionRuntimeError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityActionResult {
    pub outcome: Result<ActionSuccess, ActionError>,
    pub execution_time_ms: u64,
}

impl CapabilityActionResult {
    pub fn success(payload: Value, execution_time_ms: u64) -> Self {
        Self {
            outcome: Ok(ActionSuccess { payload }),
            execution_time_ms,
        }
    }

    pub fn input_error(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Option<Value>,
        execution_time_ms: u64,
    ) -> Self {
        Self {
            outcome: Err(ActionError::InputError(ActionInputError {
                code: code.into(),
                message: message.into(),
                details,
            })),
            execution_time_ms,
        }
    }

    pub fn runtime_error(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Option<Value>,
        execution_time_ms: u64,
    ) -> Self {
        Self {
            outcome: Err(ActionError::RuntimeError(ActionRuntimeError {
                code: code.into(),
                message: message.into(),
                details,
            })),
            execution_time_ms,
        }
    }
}
