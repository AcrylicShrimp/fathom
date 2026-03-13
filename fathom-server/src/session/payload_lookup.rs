use serde_json::Value;

use crate::history::{EXECUTION_INPUT_LOOKUP_ACTION, EXECUTION_RESULT_LOOKUP_ACTION};
use fathom_protocol::pb;

pub(crate) const LOOKUP_INJECT_MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPayloadLookup {
    pub(crate) lookup_execution_id: String,
    pub(crate) execution_id: String,
    pub(crate) part: String,
    pub(crate) offset: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) full_bytes: usize,
    pub(crate) source_truncated: bool,
    pub(crate) payload_chunk: String,
    pub(crate) injected_truncated: bool,
    pub(crate) injected_omitted_bytes: usize,
}

pub(crate) fn resolve_from_execution(execution: &pb::Execution) -> Option<ResolvedPayloadLookup> {
    let part = match execution.action_id.as_str() {
        EXECUTION_INPUT_LOOKUP_ACTION => "input",
        EXECUTION_RESULT_LOOKUP_ACTION => "result",
        _ => return None,
    };
    if execution.status != pb::ExecutionStatus::Succeeded as i32 {
        return None;
    }

    let envelope: Value = serde_json::from_str(&execution.result_message).ok()?;
    let execution_id = envelope.get("execution_id")?.as_str()?.to_string();
    let offset = value_to_usize(envelope.get("offset"))?;
    let full_bytes = value_to_usize(envelope.get("total_size"))?;
    let limit = value_to_usize(envelope.get("limit"))?;
    let payload = envelope.get("content")?.as_str()?.to_string();
    let consumed_bytes = offset.saturating_add(limit).min(full_bytes);
    let next_offset = (consumed_bytes < full_bytes).then_some(consumed_bytes);
    let (payload_chunk, injected_omitted_bytes) = truncate_utf8_by_bytes(&payload);

    Some(ResolvedPayloadLookup {
        lookup_execution_id: execution.execution_id.clone(),
        execution_id,
        part: part.to_string(),
        offset,
        next_offset,
        full_bytes,
        source_truncated: next_offset.is_some(),
        payload_chunk,
        injected_truncated: injected_omitted_bytes > 0,
        injected_omitted_bytes,
    })
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
    use super::{LOOKUP_INJECT_MAX_BYTES, resolve_from_execution};
    use crate::history::{EXECUTION_INPUT_LOOKUP_ACTION, EXECUTION_RESULT_LOOKUP_ACTION};
    use fathom_protocol::pb;

    #[test]
    fn resolve_parses_lookup_payload() {
        let execution = pb::Execution {
            execution_id: "execution-lookup-1".to_string(),
            session_id: "session-1".to_string(),
            action_id: EXECUTION_RESULT_LOOKUP_ACTION.to_string(),
            args_json: "{}".to_string(),
            status: pb::ExecutionStatus::Succeeded as i32,
            result_message: serde_json::json!({
                "execution_id": "execution-42",
                "total_size": 1024,
                "offset": 128,
                "limit": 5,
                "content": "hello"
            })
            .to_string(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };

        let resolved = resolve_from_execution(&execution).expect("lookup should resolve");
        assert_eq!(resolved.lookup_execution_id, "execution-lookup-1");
        assert_eq!(resolved.execution_id, "execution-42");
        assert_eq!(resolved.part, "result");
        assert_eq!(resolved.offset, 128);
        assert_eq!(resolved.next_offset, Some(133));
        assert_eq!(resolved.full_bytes, 1024);
        assert_eq!(resolved.payload_chunk, "hello");
        assert!(resolved.source_truncated);
    }

    #[test]
    fn resolve_truncates_injected_payload() {
        let oversized = "a".repeat(LOOKUP_INJECT_MAX_BYTES + 64);
        let execution = pb::Execution {
            execution_id: "execution-lookup-2".to_string(),
            session_id: "session-1".to_string(),
            action_id: EXECUTION_INPUT_LOOKUP_ACTION.to_string(),
            args_json: "{}".to_string(),
            status: pb::ExecutionStatus::Succeeded as i32,
            result_message: serde_json::json!({
                "execution_id": "execution-43",
                "total_size": 4242,
                "offset": 0,
                "limit": 123,
                "content": oversized
            })
            .to_string(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };

        let resolved = resolve_from_execution(&execution).expect("lookup should resolve");
        assert!(resolved.injected_truncated);
        assert!(resolved.injected_omitted_bytes > 0);
        assert_eq!(resolved.payload_chunk.len(), LOOKUP_INJECT_MAX_BYTES);
        assert_eq!(resolved.next_offset, Some(123));
        assert!(resolved.source_truncated);
    }
}
