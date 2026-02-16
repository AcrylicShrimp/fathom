use std::fs;

use serde_json::{Value, json};

use super::super::ReplaceMode;
use super::super::error::FsError;
use super::super::path::{ParsedPath, resolve_target_path};
use super::common::{map_io_error, read_utf8_file};

pub(crate) fn replace(
    path: &ParsedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
    expected_replacements: Option<usize>,
    environment_state: &Value,
) -> Result<Value, FsError> {
    if old.is_empty() {
        return Err(FsError::invalid_args("replace.old must be non-empty"));
    }

    let (_base_path, target) = resolve_target_path(environment_state, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(FsError::not_file(format!(
            "`{}` is not a file",
            path.normalized_path()
        )));
    }

    let current = read_utf8_file(&target, path.normalized_path())?;
    let replacements = match mode {
        ReplaceMode::All => current.matches(old).count(),
        ReplaceMode::First => usize::from(current.contains(old)),
    };
    if let Some(expected) = expected_replacements
        && expected != replacements
    {
        return Err(FsError::invalid_args(format!(
            "filesystem__replace expected_replacements={expected} but actual replacements={replacements}"
        )));
    }

    let updated = match mode {
        ReplaceMode::All => current.replace(old, new),
        ReplaceMode::First => current.replacen(old, new, 1),
    };

    fs::write(&target, &updated).map_err(map_io_error)?;
    Ok(json!({
        "replacements": replacements,
        "bytes": updated.len(),
    }))
}
