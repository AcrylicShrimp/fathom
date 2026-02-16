use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::error::FsError;

#[derive(Debug, Clone)]
pub(crate) struct ParsedPath {
    pub(crate) rel_path: PathBuf,
    normalized_path: String,
}

impl ParsedPath {
    pub(crate) fn target_label(&self) -> &'static str {
        "filesystem"
    }

    pub(crate) fn normalized_path(&self) -> &str {
        &self.normalized_path
    }
}

pub(crate) fn parse_path(path: &str) -> Result<ParsedPath, FsError> {
    let value = path.trim();
    if value.is_empty() {
        return Err(FsError::invalid_path(
            "path must be a non-empty relative filesystem path",
        ));
    }
    if value.contains("://") {
        return Err(FsError::invalid_path(
            "path must be a relative filesystem path without URI scheme",
        ));
    }

    let (rel_path, normalized_path) = normalize_relative(value)?;
    Ok(ParsedPath {
        rel_path,
        normalized_path,
    })
}

pub(crate) fn resolve_target_path(
    environment_state: &Value,
    rel_path: &Path,
) -> Result<(PathBuf, PathBuf), FsError> {
    let base_path = resolve_base_path(environment_state)?;
    let target = base_path.join(rel_path);
    ensure_path_stays_within_base(&base_path, &target)?;
    Ok((base_path, target))
}

pub(crate) fn resolve_base_path(environment_state: &Value) -> Result<PathBuf, FsError> {
    let raw_base = environment_state
        .as_object()
        .and_then(|state| state.get("base_path"))
        .and_then(Value::as_str)
        .unwrap_or(".");
    let raw_base = raw_base.trim();
    if raw_base.is_empty() {
        return Err(FsError::invalid_path(
            "filesystem environment base_path must be a non-empty string",
        ));
    }

    let base_path = PathBuf::from(raw_base);
    let base_path = if base_path.is_absolute() {
        base_path
    } else {
        std::env::current_dir()
            .map_err(|error| FsError::io_error(format!("failed to resolve current dir: {error}")))?
            .join(base_path)
    };

    let canonical_base = fs::canonicalize(&base_path).map_err(|error| {
        FsError::invalid_path(format!(
            "filesystem base path `{}` cannot be resolved: {error}",
            base_path.display()
        ))
    })?;
    let metadata = fs::metadata(&canonical_base).map_err(map_io_error)?;
    if !metadata.is_dir() {
        return Err(FsError::invalid_path(format!(
            "filesystem base path `{}` is not a directory",
            canonical_base.display()
        )));
    }

    Ok(canonical_base)
}

fn ensure_path_stays_within_base(base_path: &Path, target: &Path) -> Result<(), FsError> {
    let mut probe = target.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return Err(FsError::permission_denied(
                "unable to resolve path within filesystem base path",
            ));
        }
    }

    let canonical_base = fs::canonicalize(base_path).map_err(map_io_error)?;
    let canonical_probe = fs::canonicalize(&probe).map_err(map_io_error)?;
    if !canonical_probe.starts_with(&canonical_base) {
        return Err(FsError::permission_denied(
            "path escapes configured filesystem base path",
        ));
    }

    Ok(())
}

fn normalize_relative(raw: &str) -> Result<(PathBuf, String), FsError> {
    if raw.starts_with('/') || raw.starts_with('\\') || Path::new(raw).is_absolute() {
        return Err(FsError::invalid_path(
            "path must be relative to the filesystem base path",
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
                    return Err(FsError::permission_denied(
                        "path escapes filesystem base path",
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(FsError::invalid_path(
                    "path must be relative to the filesystem base path",
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

fn map_io_error(error: std::io::Error) -> FsError {
    match error.kind() {
        std::io::ErrorKind::NotFound => FsError::not_found(error.to_string()),
        std::io::ErrorKind::PermissionDenied => FsError::permission_denied(error.to_string()),
        std::io::ErrorKind::AlreadyExists => FsError::already_exists(error.to_string()),
        std::io::ErrorKind::IsADirectory => FsError::not_file(error.to_string()),
        std::io::ErrorKind::NotADirectory => FsError::not_directory(error.to_string()),
        _ => FsError::io_error(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{parse_path, resolve_target_path};

    #[test]
    fn parses_relative_path() {
        let parsed = parse_path("notes/today.md").expect("relative path should parse");
        assert_eq!(parsed.rel_path.to_string_lossy(), "notes/today.md");
        assert_eq!(parsed.normalized_path(), "notes/today.md");
    }

    #[test]
    fn rejects_uri_scheme() {
        assert!(parse_path("fs://notes.txt").is_err());
    }

    #[test]
    fn rejects_absolute_path() {
        assert!(parse_path("/tmp/file").is_err());
    }

    #[test]
    fn rejects_escape_path() {
        assert!(parse_path("../../etc/passwd").is_err());
    }

    #[test]
    fn resolves_target_with_relative_base_path() {
        let current_dir = std::env::current_dir().expect("current dir");
        let (base, target) =
            resolve_target_path(&json!({ "base_path": "." }), Path::new("Cargo.toml"))
                .expect("resolved path");

        assert_eq!(base, current_dir);
        assert_eq!(target, current_dir.join("Cargo.toml"));
    }
}
