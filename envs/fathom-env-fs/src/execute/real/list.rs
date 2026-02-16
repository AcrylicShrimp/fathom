use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use super::super::error::FsError;
use super::super::path::{ParsedPath, resolve_target_path};
use super::ListOptions;
use super::common::{is_hidden_name, map_io_error, path_for_output};

pub(crate) fn list(
    path: &ParsedPath,
    options: ListOptions,
    environment_state: &Value,
) -> Result<Value, FsError> {
    let (base_path, target) = resolve_target_path(environment_state, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_dir() {
        return Err(FsError::not_directory(format!(
            "`{}` is not a directory",
            path.normalized_path()
        )));
    }

    let mut entries = Vec::new();
    collect_dir_entries(
        &base_path,
        &target,
        options.recursive,
        options.include_hidden,
        &mut entries,
    )?;

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

    let truncated = entries.len() > options.max_entries;
    if truncated {
        entries.truncate(options.max_entries);
    }

    Ok(json!({
        "entries": entries,
        "truncated": truncated,
        "next_cursor": Value::Null,
    }))
}

fn collect_dir_entries(
    base_path: &Path,
    directory: &Path,
    recursive: bool,
    include_hidden: bool,
    entries: &mut Vec<Value>,
) -> Result<(), FsError> {
    let mut children = fs::read_dir(directory)
        .map_err(map_io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_io_error)?;
    children.sort_by_key(|entry| entry.path());

    for child in children {
        if !include_hidden && is_hidden_name(&child.file_name()) {
            continue;
        }

        let entry_path = child.path();
        let entry_type = child.file_type().map_err(map_io_error)?;
        let kind = if entry_type.is_dir() {
            "dir"
        } else if entry_type.is_file() {
            "file"
        } else {
            "other"
        };

        let rel_path = entry_path
            .strip_prefix(base_path)
            .map_err(|_| FsError::permission_denied("path escaped filesystem base path"))?;
        let rel_string = path_for_output(rel_path);
        let mut entry_json = json!({
            "path": rel_string,
            "name": child.file_name().to_string_lossy().to_string(),
            "kind": kind,
        });
        if entry_type.is_file() {
            let size = child.metadata().map_err(map_io_error)?.len();
            entry_json["size"] = json!(size);
        }
        entries.push(entry_json);

        if recursive && entry_type.is_dir() {
            collect_dir_entries(base_path, &entry_path, recursive, include_hidden, entries)?;
        }
    }

    Ok(())
}
