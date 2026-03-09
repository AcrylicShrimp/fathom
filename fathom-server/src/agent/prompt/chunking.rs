use super::util::estimate_tokens;

pub(super) fn chunk_section_messages(
    base_label: &str,
    heading: &str,
    lines: &[String],
    max_tokens: usize,
) -> Vec<(String, String)> {
    let safe_max_tokens = max_tokens.max(256);
    let mut chunks = Vec::<Vec<String>>::new();
    let mut current = vec![heading.to_string()];
    let mut current_tokens = estimate_tokens(heading);
    let continuation_heading = format!("{heading} (continued)");

    let normalized_lines = if lines.is_empty() {
        vec!["(none)".to_string()]
    } else {
        lines.to_vec()
    };

    for line in normalized_lines {
        let line_tokens = estimate_tokens(&line);
        if current.len() > 1 && current_tokens + line_tokens > safe_max_tokens {
            chunks.push(current);
            current = vec![continuation_heading.clone()];
            current_tokens = estimate_tokens(&continuation_heading);
        }
        current.push(line);
        current_tokens += line_tokens;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            let label = if index == 0 {
                base_label.to_string()
            } else {
                format!("{base_label}.{}", index + 1)
            };
            (label, chunk.join("\n"))
        })
        .collect()
}
