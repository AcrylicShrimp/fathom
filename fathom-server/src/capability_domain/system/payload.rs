use serde_json::{Value, json};

const PAYLOAD_PREVIEW_THRESHOLD_BYTES: usize = 512;
const HEAD_RATIO_NUM: usize = 3;
const HEAD_RATIO_DEN: usize = 5;

pub(super) fn preview_descriptor(payload: &str) -> Value {
    let total_size = payload.len();
    if total_size <= PAYLOAD_PREVIEW_THRESHOLD_BYTES {
        return json!({
            "total_size": total_size,
            "content": payload,
        });
    }

    let mut prefix_budget = PAYLOAD_PREVIEW_THRESHOLD_BYTES * HEAD_RATIO_NUM / HEAD_RATIO_DEN;
    if prefix_budget == 0 {
        prefix_budget = PAYLOAD_PREVIEW_THRESHOLD_BYTES;
    }
    let prefix_end = floor_char_boundary(payload, prefix_budget.min(total_size));
    let prefix = payload[..prefix_end].to_string();

    let suffix_budget = PAYLOAD_PREVIEW_THRESHOLD_BYTES.saturating_sub(prefix.len());
    let suffix_start_target = total_size.saturating_sub(suffix_budget);
    let suffix_start = ceil_char_boundary(payload, suffix_start_target.max(prefix_end));
    let suffix = if suffix_start < total_size {
        payload[suffix_start..].to_string()
    } else {
        String::new()
    };

    json!({
        "total_size": total_size,
        "prefix": prefix,
        "prefix_size": prefix_end,
        "suffix": suffix,
        "suffix_size": total_size.saturating_sub(suffix_start),
    })
}

pub(super) fn slice_payload_response(
    total_size: usize,
    offset: usize,
    limit: usize,
    content: &str,
) -> Value {
    json!({
        "total_size": total_size,
        "offset": offset,
        "limit": limit,
        "content": content,
    })
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::preview_descriptor;

    #[test]
    fn preview_descriptor_inlines_small_payloads() {
        let descriptor = preview_descriptor("{\"ok\":true}");
        assert_eq!(descriptor["content"], "{\"ok\":true}");
    }

    #[test]
    fn preview_descriptor_splits_large_payloads() {
        let descriptor = preview_descriptor(&"a".repeat(1024));
        assert!(descriptor.get("prefix").is_some());
        assert!(descriptor.get("suffix").is_some());
    }
}
