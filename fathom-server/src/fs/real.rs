use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::runtime::Runtime;

use super::ReplaceMode;
use super::error::FsError;
use super::path::RealPath;

pub(crate) fn list(runtime: &Runtime, path: &RealPath) -> Result<Value, FsError> {
    let target = resolve_real_path(runtime, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_dir() {
        return Err(FsError::not_directory(format!(
            "`{}` is not a directory",
            path.normalized_uri()
        )));
    }

    let root = runtime.workspace_root();
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
            .strip_prefix(root)
            .map_err(|_| FsError::permission_denied("path escaped workspace root"))?;
        let rel_uri = path_for_uri(rel_path);
        let mut entry_json = json!({
            "path": format!("fs://{rel_uri}"),
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

pub(crate) fn read(runtime: &Runtime, path: &RealPath) -> Result<Value, FsError> {
    let target = resolve_real_path(runtime, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(FsError::not_file(format!(
            "`{}` is not a file",
            path.normalized_uri()
        )));
    }

    let content = fs::read_to_string(&target).map_err(map_io_error)?;
    Ok(json!({
        "content": content,
        "bytes": content.len()
    }))
}

pub(crate) fn write(
    runtime: &Runtime,
    path: &RealPath,
    content: &str,
    allow_override: bool,
) -> Result<Value, FsError> {
    let target = resolve_real_path(runtime, &path.rel_path)?;

    let existed = target.exists();
    if existed {
        let metadata = fs::metadata(&target).map_err(map_io_error)?;
        if !metadata.is_file() {
            return Err(FsError::not_file(format!(
                "`{}` is not a file",
                path.normalized_uri()
            )));
        }
        if !allow_override {
            return Err(FsError::already_exists(format!(
                "`{}` already exists",
                path.normalized_uri()
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
    runtime: &Runtime,
    path: &RealPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
) -> Result<Value, FsError> {
    if old.is_empty() {
        return Err(FsError::invalid_args("replace.old must be non-empty"));
    }

    let target = resolve_real_path(runtime, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(FsError::not_file(format!(
            "`{}` is not a file",
            path.normalized_uri()
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

fn resolve_real_path(runtime: &Runtime, rel_path: &Path) -> Result<PathBuf, FsError> {
    let workspace_root = runtime.workspace_root();
    let target = workspace_root.join(rel_path);
    ensure_path_stays_within_workspace(workspace_root, &target)?;
    Ok(target)
}

fn ensure_path_stays_within_workspace(workspace_root: &Path, target: &Path) -> Result<(), FsError> {
    let mut probe = target.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return Err(FsError::permission_denied(
                "unable to resolve path within workspace root",
            ));
        }
    }

    let canonical_workspace = fs::canonicalize(workspace_root).map_err(map_io_error)?;
    let canonical_probe = fs::canonicalize(&probe).map_err(map_io_error)?;
    if !canonical_probe.starts_with(&canonical_workspace) {
        return Err(FsError::permission_denied(
            "path escapes configured workspace root",
        ));
    }

    Ok(())
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

fn path_for_uri(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        ".".to_string()
    } else {
        value
    }
}
