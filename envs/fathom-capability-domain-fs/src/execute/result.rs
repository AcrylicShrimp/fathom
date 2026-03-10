use fathom_capability_domain::ActionOutcome;
use serde_json::{Value, json};

use super::error::FsError;

pub(crate) fn success(op: &'static str, path: &str, target: &str, data: Value) -> ActionOutcome {
    ActionOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "path": path,
            "target": target,
            "data": data,
        })
        .to_string(),
        state_patch: None,
    }
}

pub(crate) fn failure(
    op: &'static str,
    path: Option<&str>,
    error: &FsError,
    target: Option<&str>,
) -> ActionOutcome {
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

    ActionOutcome {
        succeeded: false,
        message: payload.to_string(),
        state_patch: None,
    }
}
