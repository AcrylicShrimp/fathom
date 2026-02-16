use serde::Serialize;

pub(crate) const PREVIEW_MAX_BYTES: usize = 512;
pub(crate) const PREVIEW_MAX_LINES: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PayloadPreview {
    pub(crate) preview: String,
    pub(crate) full_bytes: usize,
    pub(crate) preview_bytes: usize,
    pub(crate) truncated: bool,
    pub(crate) omitted_bytes: usize,
    pub(crate) lookup_ref: String,
}

pub(crate) fn build_payload_preview(payload: &str, lookup_ref: String) -> PayloadPreview {
    let full_bytes = payload.len();
    let mut preview = String::new();
    let mut preview_bytes = 0usize;
    let mut line_count = 1usize;
    let mut truncated = false;

    for ch in payload.chars() {
        let ch_bytes = ch.len_utf8();
        if preview_bytes + ch_bytes > PREVIEW_MAX_BYTES {
            truncated = true;
            break;
        }

        if ch == '\n' && line_count >= PREVIEW_MAX_LINES {
            truncated = true;
            break;
        }

        preview.push(ch);
        preview_bytes += ch_bytes;
        if ch == '\n' {
            line_count += 1;
        }
    }

    let omitted_bytes = full_bytes.saturating_sub(preview_bytes);
    if truncated {
        preview.push_str(&format!("\n{} bytes omitted", omitted_bytes));
    }

    PayloadPreview {
        preview,
        full_bytes,
        preview_bytes,
        truncated,
        omitted_bytes,
        lookup_ref,
    }
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
        assert!(preview.preview.contains("bytes omitted"));
    }

    #[test]
    fn preview_keeps_small_payload() {
        let payload = "{\"ok\":true}";
        let preview = build_payload_preview(payload, "task://task-1/result".to_string());

        assert!(!preview.truncated);
        assert_eq!(preview.omitted_bytes, 0);
        assert_eq!(preview.preview, payload);
    }
}
