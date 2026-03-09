use serde_json::{Value, json};

use crate::pb;
use crate::runtime::Runtime;
use crate::session::execution_context::ExecutionContext;

const DEFAULT_LOOKUP_LIMIT: usize = 4096;
const MAX_LOOKUP_LIMIT: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionPayloadPart {
    Args,
    Result,
}

pub(crate) fn parse_execution_payload_part(raw: &str) -> Result<ExecutionPayloadPart, String> {
    match raw {
        "args" => Ok(ExecutionPayloadPart::Args),
        "result" => Ok(ExecutionPayloadPart::Result),
        _ => Err("part must be `args` or `result`".to_string()),
    }
}

pub(crate) async fn get_execution_payload(
    runtime: &Runtime,
    context: &ExecutionContext,
    execution_id: &str,
    part: ExecutionPayloadPart,
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    let executions = runtime
        .list_executions(&context.session_id)
        .await
        .map_err(|error| format!("failed to list executions: {}", error.message()))?;

    let execution = executions
        .into_iter()
        .find(|execution| execution.execution_id == execution_id)
        .ok_or_else(|| format!("execution `{execution_id}` not found"))?;

    let payload = match part {
        ExecutionPayloadPart::Args => execution.args_json,
        ExecutionPayloadPart::Result => execution.result_message,
    };

    let full_bytes = payload.len();
    let bounded_offset = offset.min(full_bytes);
    let max_len = normalize_lookup_limit(limit);
    let end = bounded_offset.saturating_add(max_len).min(full_bytes);
    let chunk = payload
        .as_bytes()
        .get(bounded_offset..end)
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default();

    let status = pb::ExecutionStatus::try_from(execution.status)
        .map(|status| format!("{:?}", status))
        .unwrap_or_else(|_| "UNKNOWN".to_string());

    Ok(json!({
        "execution_id": execution_id,
        "part": match part { ExecutionPayloadPart::Args => "args", ExecutionPayloadPart::Result => "result" },
        "status": status,
        "full_bytes": full_bytes,
        "offset": bounded_offset,
        "limit": max_len,
        "chunk_bytes": chunk.len(),
        "truncated": end < full_bytes,
        "next_offset": if end < full_bytes { end as i64 } else { -1 },
        "payload": chunk,
    }))
}

fn normalize_lookup_limit(limit: usize) -> usize {
    let effective = if limit == 0 {
        DEFAULT_LOOKUP_LIMIT
    } else {
        limit
    };
    effective.min(MAX_LOOKUP_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_LOOKUP_LIMIT, MAX_LOOKUP_LIMIT, normalize_lookup_limit};

    #[test]
    fn normalize_lookup_limit_defaults_when_omitted() {
        assert_eq!(normalize_lookup_limit(0), DEFAULT_LOOKUP_LIMIT);
    }

    #[test]
    fn normalize_lookup_limit_clamps_upper_bound() {
        assert_eq!(
            normalize_lookup_limit(MAX_LOOKUP_LIMIT + 1234),
            MAX_LOOKUP_LIMIT
        );
    }
}
