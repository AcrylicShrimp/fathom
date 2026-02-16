use serde_json::{Value, json};

use crate::pb;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;

const DEFAULT_LOOKUP_LIMIT: usize = 4096;
const MAX_LOOKUP_LIMIT: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPayloadPart {
    Args,
    Result,
}

pub(crate) fn parse_task_payload_part(raw: &str) -> Result<TaskPayloadPart, String> {
    match raw {
        "args" => Ok(TaskPayloadPart::Args),
        "result" => Ok(TaskPayloadPart::Result),
        _ => Err("part must be `args` or `result`".to_string()),
    }
}

pub(crate) async fn get_task_payload(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    task_id: &str,
    part: TaskPayloadPart,
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    let tasks = runtime
        .list_tasks(&context.session_id)
        .await
        .map_err(|error| format!("failed to list tasks: {}", error.message()))?;

    let task = tasks
        .into_iter()
        .find(|task| task.task_id == task_id)
        .ok_or_else(|| format!("task `{task_id}` not found"))?;

    let payload = match part {
        TaskPayloadPart::Args => task.args_json,
        TaskPayloadPart::Result => task.result_message,
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

    let status = pb::TaskStatus::try_from(task.status)
        .map(|status| format!("{:?}", status))
        .unwrap_or_else(|_| "UNKNOWN".to_string());

    Ok(json!({
        "task_id": task_id,
        "part": match part { TaskPayloadPart::Args => "args", TaskPayloadPart::Result => "result" },
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
