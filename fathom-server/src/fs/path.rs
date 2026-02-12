use std::path::{Component, Path, PathBuf};

use super::error::FsError;

const MANAGED_PREFIX: &str = "managed://";
const REAL_PREFIX: &str = "fs://";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedEntity {
    Agent,
    User,
}

impl ManagedEntity {
    fn as_str(self) -> &'static str {
        match self {
            ManagedEntity::Agent => "agent",
            ManagedEntity::User => "user",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedPath {
    pub(crate) entity: ManagedEntity,
    pub(crate) id: String,
    pub(crate) field: Option<String>,
    normalized_uri: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RealPath {
    pub(crate) rel_path: PathBuf,
    normalized_uri: String,
}

impl ManagedPath {
    pub(crate) fn normalized_uri(&self) -> &str {
        &self.normalized_uri
    }
}

impl RealPath {
    pub(crate) fn normalized_uri(&self) -> &str {
        &self.normalized_uri
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ParsedPath {
    Managed(ManagedPath),
    Real(RealPath),
}

impl ParsedPath {
    pub(crate) fn target_label(&self) -> &'static str {
        match self {
            ParsedPath::Managed(_) => "managed",
            ParsedPath::Real(_) => "fs",
        }
    }

    pub(crate) fn normalized_uri(&self) -> &str {
        match self {
            ParsedPath::Managed(path) => &path.normalized_uri,
            ParsedPath::Real(path) => &path.normalized_uri,
        }
    }
}

pub(crate) fn parse_path(path: &str) -> Result<ParsedPath, FsError> {
    if let Some(rest) = path.strip_prefix(MANAGED_PREFIX) {
        return parse_managed_path(rest);
    }
    if let Some(rest) = path.strip_prefix(REAL_PREFIX) {
        return parse_real_path(rest);
    }

    Err(FsError::invalid_path(
        "path must use managed:// or fs:// prefix",
    ))
}

fn parse_managed_path(rest: &str) -> Result<ParsedPath, FsError> {
    let segments = rest
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if !(2..=3).contains(&segments.len()) {
        return Err(FsError::invalid_path(
            "managed path must be managed://<agent|user>/<id>[/<field>]",
        ));
    }

    let entity = match segments[0] {
        "agent" => ManagedEntity::Agent,
        "user" => ManagedEntity::User,
        _ => {
            return Err(FsError::invalid_path(
                "managed path entity must be `agent` or `user`",
            ));
        }
    };

    let id = segments[1].trim();
    if id.is_empty() {
        return Err(FsError::invalid_path(
            "managed path target id must be non-empty",
        ));
    }

    let field = segments.get(2).map(|segment| segment.to_string());
    let normalized_uri = if let Some(field) = field.as_ref() {
        format!("{MANAGED_PREFIX}{}/{}/{}", entity.as_str(), id, field)
    } else {
        format!("{MANAGED_PREFIX}{}/{}", entity.as_str(), id)
    };

    Ok(ParsedPath::Managed(ManagedPath {
        entity,
        id: id.to_string(),
        field,
        normalized_uri,
    }))
}

fn parse_real_path(rest: &str) -> Result<ParsedPath, FsError> {
    let (rel_path, rel_uri) = normalize_fs_relative(rest)?;
    Ok(ParsedPath::Real(RealPath {
        rel_path,
        normalized_uri: format!("{REAL_PREFIX}{rel_uri}"),
    }))
}

fn normalize_fs_relative(raw: &str) -> Result<(PathBuf, String), FsError> {
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(FsError::invalid_path(
            "fs:// path must be workspace-relative, not absolute",
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
                        "fs:// path escapes workspace root",
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(FsError::invalid_path(
                    "fs:// path must be workspace-relative",
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

    let rel_uri = segments.join("/");
    Ok((rel_path, rel_uri))
}

#[cfg(test)]
mod tests {
    use super::{ManagedEntity, ParsedPath, parse_path};

    #[test]
    fn parses_managed_entity_root() {
        let parsed = parse_path("managed://agent/agent-a").expect("managed path should parse");
        match parsed {
            ParsedPath::Managed(path) => {
                assert_eq!(path.entity, ManagedEntity::Agent);
                assert_eq!(path.id, "agent-a");
                assert!(path.field.is_none());
            }
            ParsedPath::Real(_) => panic!("expected managed path"),
        }
    }

    #[test]
    fn parses_managed_field() {
        let parsed =
            parse_path("managed://user/user-a/long_term_memory_md").expect("managed field parse");
        match parsed {
            ParsedPath::Managed(path) => {
                assert_eq!(path.entity, ManagedEntity::User);
                assert_eq!(path.id, "user-a");
                assert_eq!(path.field.as_deref(), Some("long_term_memory_md"));
            }
            ParsedPath::Real(_) => panic!("expected managed path"),
        }
    }

    #[test]
    fn rejects_invalid_scheme() {
        assert!(parse_path("/tmp/file").is_err());
    }

    #[test]
    fn rejects_escape_path() {
        assert!(parse_path("fs://../../etc/passwd").is_err());
    }

    #[test]
    fn parses_real_path_and_normalizes() {
        let parsed = parse_path("fs://./src/../Cargo.toml").expect("real path parse");
        match parsed {
            ParsedPath::Real(path) => {
                assert_eq!(path.rel_path.to_string_lossy(), "Cargo.toml");
            }
            ParsedPath::Managed(_) => panic!("expected real path"),
        }
    }
}
