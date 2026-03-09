use crate::history::PayloadPreview;

use super::{MAX_PREVIEW_HEAD_CHARS, MAX_PREVIEW_TAIL_CHARS, TOKEN_DIVISOR_CHARS};

pub(super) fn preview_to_inline(preview: &PayloadPreview) -> String {
    let head = truncate_inline(&preview.head, MAX_PREVIEW_HEAD_CHARS);
    let tail = truncate_inline(&preview.tail, MAX_PREVIEW_TAIL_CHARS);
    format!(
        "lookup_ref={} full_bytes={} omitted_bytes={} truncated={} head={} tail={}",
        preview.lookup_ref,
        preview.full_bytes,
        preview.omitted_bytes,
        preview.truncated,
        head,
        tail
    )
}

pub(super) fn estimate_tokens(text: &str) -> usize {
    (text.chars().count().saturating_add(TOKEN_DIVISOR_CHARS - 1)) / TOKEN_DIVISOR_CHARS
}

pub(super) fn truncate_inline(input: &str, max_chars: usize) -> String {
    let sanitized = input.replace('\n', "\\n").replace('\r', "\\r");
    let total = sanitized.chars().count();
    if total <= max_chars {
        return sanitized;
    }
    let prefix = sanitized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    format!("{prefix}...")
}

pub(super) fn serialize_pretty_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

pub(super) fn read_usize_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(super) fn read_ratio_env(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .filter(|value| *value > 0.0 && *value <= 1.0)
        .unwrap_or(default)
}
