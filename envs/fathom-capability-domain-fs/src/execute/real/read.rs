use std::cmp::min;
use std::fs;

use serde_json::{Value, json};

use super::super::error::FsError;
use super::super::path::{ParsedPath, resolve_target_path};
use super::ReadOptions;
use super::common::{map_io_error, read_utf8_file};

pub(crate) fn read(
    path: &ParsedPath,
    options: ReadOptions,
    capability_domain_state: &Value,
) -> Result<Value, FsError> {
    let (_base_path, target) = resolve_target_path(capability_domain_state, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(FsError::not_file(format!(
            "`{}` is not a file",
            path.normalized_path()
        )));
    }

    let text = read_utf8_file(&target, path.normalized_path())?;
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();
    let start_index = options.offset_line.saturating_sub(1);

    let (content, returned_lines) = if start_index >= total_lines {
        (String::new(), 0usize)
    } else {
        let end_index = min(total_lines, start_index.saturating_add(options.limit_lines));
        (
            lines[start_index..end_index].join("\n"),
            end_index - start_index,
        )
    };

    Ok(json!({
        "content": content,
        "start_line": options.offset_line,
        "returned_lines": returned_lines,
        "total_lines": total_lines,
        "truncated": start_index.saturating_add(returned_lines) < total_lines,
        "bytes": text.len(),
    }))
}
