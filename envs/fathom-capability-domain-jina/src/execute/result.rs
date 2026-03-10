use fathom_capability_domain::ActionOutcome;
use serde_json::{Value, json};

use super::error::JinaError;

pub(crate) fn success(
    op: &'static str,
    data: Value,
    attempts: Vec<Value>,
    selected_attempt_index: usize,
    used_fallback: bool,
    advisory: &str,
) -> ActionOutcome {
    ActionOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "target": "jina",
            "data": data,
            "attempts": attempts,
            "selected_attempt_index": selected_attempt_index,
            "used_fallback": used_fallback,
            "advisory": advisory,
        })
        .to_string(),
        state_patch: None,
    }
}

pub(crate) fn failure(
    op: &'static str,
    error: &JinaError,
    data: Option<Value>,
    attempts: Vec<Value>,
    advisory: &str,
) -> ActionOutcome {
    let mut payload = json!({
        "ok": false,
        "op": op,
        "target": "jina",
        "attempts": attempts,
        "advisory": advisory,
        "error": {
            "code": error.code(),
            "message": error.message(),
        },
    });

    if let Some(data) = data {
        payload["data"] = data;
    }
    if let Some(details) = error.details() {
        payload["error"]["details"] = details.clone();
    }

    ActionOutcome {
        succeeded: false,
        message: payload.to_string(),
        state_patch: None,
    }
}
