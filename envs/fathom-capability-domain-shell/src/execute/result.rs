use fathom_capability_domain::ActionOutcome;
use serde_json::{Value, json};

use super::error::ShellError;

pub(crate) fn success(op: &'static str, path: &str, data: Value) -> ActionOutcome {
    ActionOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "path": path,
            "target": "shell",
            "data": data,
        })
        .to_string(),
        state_patch: None,
    }
}

pub(crate) fn failure(
    op: &'static str,
    path: Option<&str>,
    error: &ShellError,
    data: Option<Value>,
) -> ActionOutcome {
    let mut payload = json!({
        "ok": false,
        "op": op,
        "target": "shell",
        "error": {
            "code": error.code(),
            "message": error.message(),
        },
    });

    if let Some(path) = path {
        payload["path"] = json!(path);
    }
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
