use serde_json::{Value, json};

use super::error::FsError;

#[derive(Debug, Clone)]
pub(crate) struct TaskOutcome {
    pub(crate) succeeded: bool,
    pub(crate) message: String,
}

pub(crate) fn success(op: &'static str, path: &str, target: &str, data: Value) -> TaskOutcome {
    TaskOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "path": path,
            "target": target,
            "data": data,
        })
        .to_string(),
    }
}

pub(crate) fn failure(
    op: &'static str,
    path: Option<&str>,
    error: &FsError,
    target: Option<&str>,
) -> TaskOutcome {
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

    TaskOutcome {
        succeeded: false,
        message: payload.to_string(),
    }
}
