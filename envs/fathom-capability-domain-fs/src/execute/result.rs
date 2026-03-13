use fathom_capability_domain::CapabilityActionResult;
use serde_json::{Value, json};

use super::error::FsError;

pub(crate) fn success(
    op: &'static str,
    path: &str,
    target: &str,
    data: Value,
) -> CapabilityActionResult {
    CapabilityActionResult::success(
        json!({
            "ok": true,
            "op": op,
            "path": path,
            "target": target,
            "data": data,
        }),
        0,
    )
}

pub(crate) fn failure(
    op: &'static str,
    path: Option<&str>,
    error: &FsError,
    target: Option<&str>,
) -> CapabilityActionResult {
    let mut payload = json!({
        "ok": false,
        "op": op,
        "error_code": error.code(),
        "message": error.message(),
    });

    if let Some(path) = path {
        payload["path"] = json!(path);
    }
    if let Some(target) = target {
        payload["target"] = json!(target);
    }

    if error.code() == "invalid_args" {
        CapabilityActionResult::input_error(error.code(), error.message(), Some(payload), 0)
    } else {
        CapabilityActionResult::runtime_error(error.code(), error.message(), Some(payload), 0)
    }
}
