use fathom_capability_domain::CapabilityActionResult;
use serde_json::{Value, json};

use super::error::BraveError;

pub(crate) fn success(op: &'static str, data: Value) -> CapabilityActionResult {
    CapabilityActionResult::success(
        json!({
            "ok": true,
            "op": op,
            "target": "brave_search",
            "data": data,
        }),
        0,
    )
}

pub(crate) fn failure(
    op: &'static str,
    error: &BraveError,
    data: Option<Value>,
) -> CapabilityActionResult {
    let mut payload = json!({
        "ok": false,
        "op": op,
        "target": "brave_search",
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
