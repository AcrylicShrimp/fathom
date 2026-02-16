use fathom_env::ActionOutcome;
use serde_json::{Value, json};

use super::error::JinaError;

pub(crate) fn success(op: &'static str, data: Value) -> ActionOutcome {
    ActionOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "target": "jina",
            "data": data,
        })
        .to_string(),
        state_patch: None,
    }
}

pub(crate) fn failure(op: &'static str, error: &JinaError, data: Option<Value>) -> ActionOutcome {
    let mut payload = json!({
        "ok": false,
        "op": op,
        "target": "jina",
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
