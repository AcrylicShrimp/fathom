use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;
use regex::RegexBuilder;
use serde_json::{Value, json};

use super::super::error::FsError;
use super::super::path::{ParsedPath, resolve_target_path};
use super::SearchOptions;
use super::common::{is_hidden_name, map_io_error, path_for_output, read_utf8_file};

pub(crate) fn search(
    path: &ParsedPath,
    pattern: &str,
    options: SearchOptions,
    environment_state: &Value,
) -> Result<Value, FsError> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(FsError::invalid_args(
            "filesystem__search.pattern must be a non-empty string",
        ));
    }

    let regex = RegexBuilder::new(pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|error| FsError::invalid_args(format!("invalid regex pattern: {error}")))?;
    let include_patterns = compile_include_patterns(&options.include)?;

    let (base_path, target) = resolve_target_path(environment_state, &path.rel_path)?;
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
        collect_files_recursive(&target, &mut candidates)?;
    } else {
        candidates.push(target.clone());
    }

    let mut matches = Vec::new();
    let mut truncated = false;

    for candidate in candidates {
        let rel_base = candidate
            .strip_prefix(&base_path)
            .map_err(|_| FsError::permission_denied("path escaped filesystem base path"))?;
        let rel_base_string = path_for_output(rel_base);
        if !include_patterns.is_empty()
            && !include_patterns
                .iter()
                .any(|glob| glob.matches(&rel_base_string))
        {
            continue;
        }

        let text = read_utf8_file(&candidate, &rel_base_string)?;
        for (line_index, line) in text.lines().enumerate() {
            for mat in regex.find_iter(line) {
                matches.push(json!({
                    "path": rel_base_string.clone(),
                    "line": line_index + 1,
                    "column": mat.start() + 1,
                    "preview": line,
                }));
                if matches.len() >= options.max_results {
                    truncated = true;
                    break;
                }
            }
            if truncated {
                break;
            }
        }
        if truncated {
            break;
        }
    }

    Ok(json!({
        "matches": matches,
        "truncated": truncated,
    }))
}

fn compile_include_patterns(raw_patterns: &[String]) -> Result<Vec<Pattern>, FsError> {
    let mut patterns = Vec::with_capacity(raw_patterns.len());
    for raw in raw_patterns {
        let value = raw.trim();
        if value.is_empty() {
            return Err(FsError::invalid_args(
                "filesystem__search.include entries must be non-empty strings",
            ));
        }
        let pattern = Pattern::new(value).map_err(|error| {
            FsError::invalid_args(format!("invalid include glob pattern `{value}`: {error}"))
        })?;
        patterns.push(pattern);
    }
    Ok(patterns)
}

fn collect_files_recursive(directory: &Path, out: &mut Vec<PathBuf>) -> Result<(), FsError> {
    let mut children = fs::read_dir(directory)
        .map_err(map_io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_io_error)?;
    children.sort_by_key(|entry| entry.path());

    for child in children {
        if is_hidden_name(&child.file_name()) {
            continue;
        }
        let entry_type = child.file_type().map_err(map_io_error)?;
        let entry_path = child.path();
        if entry_type.is_dir() {
            collect_files_recursive(&entry_path, out)?;
        } else if entry_type.is_file() {
            out.push(entry_path);
        }
    }
    Ok(())
}
