use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Value, json};

use super::ReplaceMode;
use super::error::FsError;
use super::path::{ParsedPath, resolve_target_path};

pub(crate) fn list(path: &ParsedPath, environment_state: &Value) -> Result<Value, FsError> {
    let (base_path, target) = resolve_target_path(environment_state, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_dir() {
        return Err(FsError::not_directory(format!(
            "`{}` is not a directory",
            path.normalized_path()
        )));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&target).map_err(map_io_error)? {
        let entry = entry.map_err(map_io_error)?;
        let entry_path = entry.path();
        let entry_type = entry.file_type().map_err(map_io_error)?;
        let kind = if entry_type.is_dir() {
            "dir"
        } else if entry_type.is_file() {
            "file"
        } else {
            "other"
        };

        let rel_path = entry_path
            .strip_prefix(&base_path)
            .map_err(|_| FsError::permission_denied("path escaped filesystem base path"))?;
        let rel_string = path_for_output(rel_path);
        let mut entry_json = json!({
            "path": rel_string,
            "name": entry.file_name().to_string_lossy(),
            "kind": kind,
        });

        if entry_type.is_file() {
            let size = entry.metadata().map_err(map_io_error)?.len();
            entry_json["size"] = json!(size);
        }

        entries.push(entry_json);
    }

    entries.sort_by(|a, b| {
        let a = a
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let b = b
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        a.cmp(&b)
    });

    Ok(json!({ "entries": entries }))
}

pub(crate) fn read(path: &ParsedPath, environment_state: &Value) -> Result<Value, FsError> {
    let (_base_path, target) = resolve_target_path(environment_state, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(FsError::not_file(format!(
            "`{}` is not a file",
            path.normalized_path()
        )));
    }

    let content = fs::read_to_string(&target).map_err(map_io_error)?;
    Ok(json!({
        "content": content,
        "bytes": content.len()
    }))
}

pub(crate) fn write(
    path: &ParsedPath,
    content: &str,
    allow_override: bool,
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
        fs::create_dir_all(parent).map_err(map_io_error)?;
    }

    fs::write(&target, content).map_err(map_io_error)?;
    Ok(json!({
        "bytes_written": content.len(),
        "created": !existed,
        "overwritten": existed
    }))
}

pub(crate) fn replace(
    path: &ParsedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
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

    let current = fs::read_to_string(&target).map_err(map_io_error)?;
    let (updated, replacements) = match mode {
        ReplaceMode::All => {
            let replacements = current.matches(old).count();
            (current.replace(old, new), replacements)
        }
        ReplaceMode::First => {
            if let Some(start) = current.find(old) {
                if start == 0 && old.len() == current.len() {
                    (new.to_string(), 1)
                } else {
                    let mut updated = String::with_capacity(current.len() - old.len() + new.len());
                    updated.push_str(&current[..start]);
                    updated.push_str(new);
                    updated.push_str(&current[start + old.len()..]);
                    (updated, 1)
                }
            } else {
                (current, 0)
            }
        }
    };

    fs::write(&target, &updated).map_err(map_io_error)?;
    Ok(json!({
        "replacements": replacements,
        "bytes": updated.len()
    }))
}

fn map_io_error(error: io::Error) -> FsError {
    match error.kind() {
        io::ErrorKind::NotFound => FsError::not_found(error.to_string()),
        io::ErrorKind::PermissionDenied => FsError::permission_denied(error.to_string()),
        io::ErrorKind::AlreadyExists => FsError::already_exists(error.to_string()),
        io::ErrorKind::IsADirectory => FsError::not_file(error.to_string()),
        io::ErrorKind::NotADirectory => FsError::not_directory(error.to_string()),
        _ => FsError::io_error(error.to_string()),
    }
}

fn path_for_output(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        ".".to_string()
    } else {
        value
    }
}
