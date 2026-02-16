use std::fs;

use serde_json::{Value, json};

use super::super::error::FsError;
use super::super::path::{ParsedPath, resolve_target_path};
use super::common::map_io_error;

pub(crate) fn write(
    path: &ParsedPath,
    content: &str,
    allow_override: bool,
    create_parents: bool,
    environment_state: &Value,
) -> Result<Value, FsError> {
    let (_base_path, target) = resolve_target_path(environment_state, &path.rel_path)?;

    let existed = target.exists();
    if existed {
        let metadata = fs::metadata(&target).map_err(map_io_error)?;
        if !metadata.is_file() {
            return Err(FsError::not_file(format!(
                "`{}` is not a file",
                path.normalized_path()
            )));
        }
        if !allow_override {
            return Err(FsError::already_exists(format!(
                "`{}` already exists",
                path.normalized_path()
            )));
        }
    }

    if let Some(parent) = target.parent() {
        if parent.exists() {
            let parent_metadata = fs::metadata(parent).map_err(map_io_error)?;
            if !parent_metadata.is_dir() {
                return Err(FsError::not_directory(format!(
                    "parent path for `{}` is not a directory",
                    path.normalized_path()
                )));
            }
        } else if create_parents {
            fs::create_dir_all(parent).map_err(map_io_error)?;
        } else {
            return Err(FsError::not_found(format!(
                "parent directory for `{}` does not exist",
                path.normalized_path()
            )));
        }
    }

    fs::write(&target, content).map_err(map_io_error)?;
    Ok(json!({
        "bytes_written": content.len(),
        "created": !existed,
        "overwritten": existed,
    }))
}
