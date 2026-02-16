use serde_json::Value;

use crate::history::TASK_PAYLOAD_LOOKUP_ACTION;
use crate::pb;

pub(crate) const LOOKUP_INJECT_MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPayloadLookup {
    pub(crate) lookup_task_id: String,
    pub(crate) task_id: String,
    pub(crate) part: String,
    pub(crate) offset: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) full_bytes: usize,
    pub(crate) source_truncated: bool,
    pub(crate) payload_chunk: String,
    pub(crate) injected_truncated: bool,
    pub(crate) injected_omitted_bytes: usize,
}

pub(crate) fn resolve_from_task(task: &pb::Task) -> Option<ResolvedPayloadLookup> {
    if task.action_id != TASK_PAYLOAD_LOOKUP_ACTION {
        return None;
    }
    if task.status != pb::TaskStatus::Succeeded as i32 {
        return None;
    }

    let envelope: Value = serde_json::from_str(&task.result_message).ok()?;
    if !envelope.get("ok")?.as_bool()? {
        return None;
    }
    if envelope.get("op")?.as_str()? != TASK_PAYLOAD_LOOKUP_ACTION {
        return None;
    }

    let data = envelope.get("data")?;
    let task_id = data.get("task_id")?.as_str()?.to_string();
    let part = data.get("part")?.as_str()?.to_string();
    let offset = value_to_usize(data.get("offset"))?;
    let full_bytes = value_to_usize(data.get("full_bytes"))?;
    let source_truncated = data
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next_offset = parse_next_offset(data.get("next_offset"))?;
    let payload = data.get("payload")?.as_str()?.to_string();
    let (payload_chunk, injected_omitted_bytes) = truncate_utf8_by_bytes(&payload);

    Some(ResolvedPayloadLookup {
        lookup_task_id: task.task_id.clone(),
        task_id,
        part,
        offset,
        next_offset,
        full_bytes,
        source_truncated,
        injected_truncated: injected_omitted_bytes > 0,
        injected_omitted_bytes,
        payload_chunk,
    })
}

fn parse_next_offset(value: Option<&Value>) -> Option<Option<usize>> {
    let Some(value) = value else {
        return Some(None);
    };
    let next = value.as_i64()?;
    if next < 0 {
        return Some(None);
    }
    usize::try_from(next).ok().map(Some)
}

fn value_to_usize(value: Option<&Value>) -> Option<usize> {
    let value = value?;
    value.as_u64().and_then(|raw| usize::try_from(raw).ok())
}

fn truncate_utf8_by_bytes(value: &str) -> (String, usize) {
    if value.len() <= LOOKUP_INJECT_MAX_BYTES {
        return (value.to_string(), 0);
    }

    let mut end = LOOKUP_INJECT_MAX_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let omitted = value.len().saturating_sub(end);
    (value[..end].to_string(), omitted)
}

#[cfg(test)]
mod tests {
    use super::{LOOKUP_INJECT_MAX_BYTES, resolve_from_task};
    use crate::history::TASK_PAYLOAD_LOOKUP_ACTION;
    use crate::pb;

    #[test]
    fn resolve_parses_lookup_payload() {
        let task = pb::Task {
            task_id: "task-lookup-1".to_string(),
            session_id: "session-1".to_string(),
            action_id: TASK_PAYLOAD_LOOKUP_ACTION.to_string(),
            args_json: "{}".to_string(),
            status: pb::TaskStatus::Succeeded as i32,
            result_message: serde_json::json!({
                "ok": true,
                "op": TASK_PAYLOAD_LOOKUP_ACTION,
                "data": {
                    "task_id": "task-42",
                    "part": "result",
                    "offset": 128,
                    "next_offset": -1,
                    "full_bytes": 1024,
                    "truncated": false,
                    "payload": "hello"
                }
            })
            .to_string(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };

        let resolved = resolve_from_task(&task).expect("lookup should resolve");
        assert_eq!(resolved.lookup_task_id, "task-lookup-1");
        assert_eq!(resolved.task_id, "task-42");
        assert_eq!(resolved.part, "result");
        assert_eq!(resolved.offset, 128);
        assert_eq!(resolved.next_offset, None);
        assert_eq!(resolved.full_bytes, 1024);
        assert_eq!(resolved.payload_chunk, "hello");
        assert!(!resolved.injected_truncated);
    }

    #[test]
    fn resolve_truncates_injected_payload() {
        let oversized = "a".repeat(LOOKUP_INJECT_MAX_BYTES + 64);
        let task = pb::Task {
            task_id: "task-lookup-2".to_string(),
            session_id: "session-1".to_string(),
            action_id: TASK_PAYLOAD_LOOKUP_ACTION.to_string(),
            args_json: "{}".to_string(),
            status: pb::TaskStatus::Succeeded as i32,
            result_message: serde_json::json!({
                "ok": true,
                "op": TASK_PAYLOAD_LOOKUP_ACTION,
                "data": {
                    "task_id": "task-43",
                    "part": "args",
                    "offset": 0,
                    "next_offset": 123,
                    "full_bytes": 4242,
                    "truncated": true,
                    "payload": oversized
                }
            })
            .to_string(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };

        let resolved = resolve_from_task(&task).expect("lookup should resolve");
        assert!(resolved.injected_truncated);
        assert!(resolved.injected_omitted_bytes > 0);
        assert_eq!(resolved.payload_chunk.len(), LOOKUP_INJECT_MAX_BYTES);
        assert_eq!(resolved.next_offset, Some(123));
        assert!(resolved.source_truncated);
    }
}
