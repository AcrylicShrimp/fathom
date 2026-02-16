use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::error::ShellError;

#[derive(Debug, Clone)]
pub(crate) struct ParsedPath {
    pub(crate) rel_path: PathBuf,
    normalized_path: String,
}

impl ParsedPath {
    pub(crate) fn normalized_path(&self) -> &str {
        &self.normalized_path
    }
}

pub(crate) fn parse_path(path: &str) -> Result<ParsedPath, ShellError> {
    let value = path.trim();
    if value.is_empty() {
        return Err(ShellError::invalid_path(
            "path must be a non-empty relative filesystem path",
        ));
    }
    if value.contains("://") {
        return Err(ShellError::invalid_path(
            "path must be a relative filesystem path without URI scheme",
        ));
    }

    let (rel_path, normalized_path) = normalize_relative(value)?;
    Ok(ParsedPath {
        rel_path,
        normalized_path,
    })
}

pub(crate) fn resolve_target_dir(
    environment_state: &Value,
    rel_path: &Path,
) -> Result<(PathBuf, PathBuf), ShellError> {
    let base_path = resolve_base_path(environment_state)?;
    let target = base_path.join(rel_path);
    ensure_path_stays_within_base(&base_path, &target)?;

    let metadata = fs::metadata(&target).map_err(map_io_error)?;
    if !metadata.is_dir() {
        return Err(ShellError::not_directory(format!(
            "path `{}` is not a directory",
            target.display()
        )));
    }

    Ok((base_path, target))
}

fn resolve_base_path(environment_state: &Value) -> Result<PathBuf, ShellError> {
    let raw_base = environment_state
        .as_object()
        .and_then(|state| state.get("base_path"))
        .and_then(Value::as_str)
        .unwrap_or(".");
    let raw_base = raw_base.trim();
    if raw_base.is_empty() {
        return Err(ShellError::invalid_path(
            "shell environment base_path must be a non-empty string",
        ));
    }

    let base_path = PathBuf::from(raw_base);
    let base_path = if base_path.is_absolute() {
        base_path
    } else {
        std::env::current_dir()
            .map_err(|error| {
                ShellError::io_error(format!("failed to resolve current dir: {error}"))
            })?
            .join(base_path)
    };

    let canonical_base = fs::canonicalize(&base_path).map_err(|error| {
        ShellError::invalid_path(format!(
            "shell base path `{}` cannot be resolved: {error}",
            base_path.display()
        ))
    })?;
    let metadata = fs::metadata(&canonical_base).map_err(map_io_error)?;
    if !metadata.is_dir() {
        return Err(ShellError::invalid_path(format!(
            "shell base path `{}` is not a directory",
            canonical_base.display()
        )));
    }

    Ok(canonical_base)
}

fn ensure_path_stays_within_base(base_path: &Path, target: &Path) -> Result<(), ShellError> {
    let mut probe = target.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return Err(ShellError::permission_denied(
                "unable to resolve path within shell base path",
            ));
        }
    }

    let canonical_base = fs::canonicalize(base_path).map_err(map_io_error)?;
    let canonical_probe = fs::canonicalize(&probe).map_err(map_io_error)?;
    if !canonical_probe.starts_with(&canonical_base) {
        return Err(ShellError::permission_denied(
            "path escapes configured shell base path",
        ));
    }

    Ok(())
}

fn normalize_relative(raw: &str) -> Result<(PathBuf, String), ShellError> {
    if raw.starts_with('/') || raw.starts_with('\\') || Path::new(raw).is_absolute() {
        return Err(ShellError::invalid_path(
            "path must be relative to the shell base path",
        ));
    }

    let mut segments: Vec<String> = Vec::new();
    for component in Path::new(raw).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => {
                segments.push(segment.to_string_lossy().to_string());
            }
            Component::ParentDir => {
                if segments.pop().is_none() {
                    return Err(ShellError::permission_denied(
                        "path escapes shell base path",
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ShellError::invalid_path(
                    "path must be relative to the shell base path",
                ));
            }
        }
    }

    let mut rel_path = PathBuf::new();
    for segment in &segments {
        rel_path.push(segment);
    }

    if rel_path.as_os_str().is_empty() {
        return Ok((PathBuf::from("."), ".".to_string()));
    }

    let normalized_path = segments.join("/");
    Ok((rel_path, normalized_path))
}

fn map_io_error(error: std::io::Error) -> ShellError {
    match error.kind() {
        std::io::ErrorKind::NotFound => ShellError::not_found(error.to_string()),
        std::io::ErrorKind::PermissionDenied => ShellError::permission_denied(error.to_string()),
        std::io::ErrorKind::NotADirectory => ShellError::not_directory(error.to_string()),
        _ => ShellError::io_error(error.to_string()),
    }
}
