use fathom_capability_domain::CapabilityActionResult;
use serde_json::{Value, json};

use super::error::JinaError;

pub(crate) fn success(
    op: &'static str,
    data: Value,
    attempts: Vec<Value>,
    selected_attempt_index: usize,
    used_fallback: bool,
    advisory: &str,
) -> CapabilityActionResult {
    CapabilityActionResult::success(
        json!({
            "ok": true,
            "op": op,
            "target": "jina",
            "data": data,
            "attempts": attempts,
            "selected_attempt_index": selected_attempt_index,
            "used_fallback": used_fallback,
            "advisory": advisory,
        }),
        0,
    )
}

pub(crate) fn failure(
    op: &'static str,
    error: &JinaError,
    data: Option<Value>,
    attempts: Vec<Value>,
    advisory: &str,
) -> CapabilityActionResult {
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

    if error.code() == "invalid_args" {
        CapabilityActionResult::input_error(error.code(), error.message(), Some(payload), 0)
    } else {
        CapabilityActionResult::runtime_error(error.code(), error.message(), Some(payload), 0)
    }
}
