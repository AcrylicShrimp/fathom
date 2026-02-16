use serde::Serialize;

pub(crate) const PREVIEW_MAX_BYTES: usize = 512;
pub(crate) const PREVIEW_MAX_LINES: usize = 8;
const PREVIEW_HEAD_RATIO_NUM: usize = 3;
const PREVIEW_HEAD_RATIO_DEN: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PayloadPreview {
    pub(crate) head: String,
    pub(crate) tail: String,
    pub(crate) full_bytes: usize,
    pub(crate) head_bytes: usize,
    pub(crate) tail_bytes: usize,
    pub(crate) truncated: bool,
    pub(crate) omitted_bytes: usize,
    pub(crate) lookup_ref: String,
}

pub(crate) fn build_payload_preview(payload: &str, lookup_ref: String) -> PayloadPreview {
    let full_bytes = payload.len();

    let full_visible_end = head_end_index(payload, PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES);
    if full_visible_end == full_bytes {
        return PayloadPreview {
            head: payload.to_string(),
            tail: String::new(),
            full_bytes,
            head_bytes: full_bytes,
            tail_bytes: 0,
            truncated: false,
            omitted_bytes: 0,
            lookup_ref,
        };
    }

    let mut head_budget = PREVIEW_MAX_BYTES * PREVIEW_HEAD_RATIO_NUM / PREVIEW_HEAD_RATIO_DEN;
    if head_budget == 0 {
        head_budget = PREVIEW_MAX_BYTES;
    }
    let head_end = head_end_index(payload, head_budget, PREVIEW_MAX_LINES);
    let head = payload[..head_end].to_string();
    let head_bytes = head.len();

    let tail_budget = PREVIEW_MAX_BYTES.saturating_sub(head_bytes);
    let tail_start_target = full_bytes.saturating_sub(tail_budget);
    let mut tail_start = tail_start_target.max(head_bytes);
    while tail_start < full_bytes && !payload.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let tail = if tail_start < full_bytes {
        payload[tail_start..].to_string()
    } else {
        String::new()
    };
    let tail_bytes = tail.len();
    let omitted_bytes = full_bytes.saturating_sub(head_bytes.saturating_add(tail_bytes));

    PayloadPreview {
        head,
        tail,
        full_bytes,
        head_bytes,
        tail_bytes,
        truncated: omitted_bytes > 0,
        omitted_bytes,
        lookup_ref,
    }
}

fn head_end_index(payload: &str, max_bytes: usize, max_lines: usize) -> usize {
    let mut head_end = 0usize;
    let mut head_bytes = 0usize;
    let mut line_count = 1usize;

    for ch in payload.chars() {
        let ch_bytes = ch.len_utf8();
        if head_bytes + ch_bytes > max_bytes {
            break;
        }

        if ch == '\n' && line_count >= max_lines {
            break;
        }

        head_end += ch_bytes;
        head_bytes += ch_bytes;
        if ch == '\n' {
            line_count += 1;
        }
    }

    head_end
}

#[cfg(test)]
mod tests {
    use super::{PREVIEW_MAX_BYTES, build_payload_preview};

    #[test]
    fn preview_truncates_large_payload() {
        let payload = "a".repeat(PREVIEW_MAX_BYTES + 128);
        let preview = build_payload_preview(&payload, "task://task-1/args".to_string());

        assert!(preview.truncated);
        assert!(preview.omitted_bytes > 0);
        assert!(!preview.head.is_empty());
        assert!(!preview.tail.is_empty());
    }

    #[test]
    fn preview_keeps_small_payload() {
        let payload = "{\"ok\":true}";
        let preview = build_payload_preview(payload, "task://task-1/result".to_string());

        assert!(!preview.truncated);
        assert_eq!(preview.omitted_bytes, 0);
        assert_eq!(preview.head, payload);
        assert!(preview.tail.is_empty());
    }

    #[test]
    fn preview_preserves_utf8_boundaries_for_head_and_tail() {
        let payload = format!("{}\n{}", "한글".repeat(300), "끝".repeat(120));
        let preview = build_payload_preview(&payload, "task://task-2/result".to_string());

        assert!(preview.head.is_char_boundary(preview.head.len()));
        assert!(preview.tail.is_char_boundary(preview.tail.len()));
    }
}
