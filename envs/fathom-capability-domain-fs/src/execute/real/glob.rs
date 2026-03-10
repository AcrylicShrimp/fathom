use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;
use serde_json::{Value, json};

use super::super::error::FsError;
use super::super::path::{ParsedPath, resolve_target_path};
use super::GlobOptions;
use super::common::{is_hidden_name, map_io_error, path_for_output};

pub(crate) fn glob(
    path: &ParsedPath,
    pattern: &str,
    options: GlobOptions,
    capability_domain_state: &Value,
) -> Result<Value, FsError> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(FsError::invalid_args(
            "filesystem__glob.pattern must be a non-empty string",
        ));
    }
    let compiled = Pattern::new(pattern).map_err(|error| {
        FsError::invalid_args(format!("invalid glob pattern `{pattern}`: {error}"))
    })?;

    let (base_path, target) = resolve_target_path(capability_domain_state, &path.rel_path)?;
    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    let target_is_dir = metadata.is_dir();
    if !target_is_dir && !metadata.is_file() {
        return Err(FsError::not_file(format!(
            "`{}` is not a file",
            path.normalized_path()
        )));
    }

    let mut candidates = Vec::new();
    if target_is_dir {
        collect_files_recursive(&target, options.include_hidden, &mut candidates)?;
    } else {
        candidates.push(target.clone());
    }

    let mut matches = Vec::new();
    for candidate in candidates {
        let rel_base = candidate
            .strip_prefix(&base_path)
            .map_err(|_| FsError::permission_denied("path escaped filesystem base path"))?;
        let rel_base_string = path_for_output(rel_base);
        let rel_scope_string = if target_is_dir {
            let rel_scope = candidate
                .strip_prefix(&target)
                .map_err(|_| FsError::permission_denied("path escaped filesystem base path"))?;
            path_for_output(rel_scope)
        } else {
            candidate
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_base_string.clone())
        };

        if compiled.matches(&rel_scope_string) || compiled.matches(&rel_base_string) {
            matches.push(rel_base_string);
        }
    }

    matches.sort();
    matches.dedup();
    let truncated = matches.len() > options.max_results;
    if truncated {
        matches.truncate(options.max_results);
    }

    Ok(json!({
        "matches": matches,
        "truncated": truncated,
    }))
}

fn collect_files_recursive(
    directory: &Path,
    include_hidden: bool,
    out: &mut Vec<PathBuf>,
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
        let entry_type = child.file_type().map_err(map_io_error)?;
        let entry_path = child.path();
        if entry_type.is_dir() {
            collect_files_recursive(&entry_path, include_hidden, out)?;
        } else if entry_type.is_file() {
            out.push(entry_path);
        }
    }

    Ok(())
}
