use serde_json::{Value, json};
use tonic::Code;

use crate::runtime::Runtime;

use super::ReplaceMode;
use super::error::FsError;
use super::path::{ManagedEntity, ManagedPath};

const AGENT_FIELDS: &[&str] = &[
    "agents_md",
    "soul_md",
    "identity_md",
    "guidelines_md",
    "code_of_conduct_md",
    "long_term_memory_md",
];

const USER_FIELDS: &[&str] = &[
    "user_md",
    "preferences_json",
    "long_term_memory_md",
    "name",
    "nickname",
];

pub(crate) async fn list(runtime: &Runtime, path: &ManagedPath) -> Result<Value, FsError> {
    match path.entity {
        ManagedEntity::Agent => {
            let _ = runtime.get_or_create_agent_profile(&path.id).await;
        }
        ManagedEntity::User => {
            let _ = runtime.get_or_create_user_profile(&path.id).await;
        }
    }

    if let Some(field) = path.field.as_deref() {
        validate_field(path.entity, field)?;
        return Ok(json!({
            "entries": [
                {
                    "path": path.normalized_uri(),
                    "name": field,
                    "kind": "file"
                }
            ]
        }));
    }

    let base = path.normalized_uri();
    let entries = allowed_fields(path.entity)
        .iter()
        .map(|field| {
            json!({
                "path": format!("{base}/{field}"),
                "name": field,
                "kind": "file"
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({ "entries": entries }))
}

pub(crate) async fn read(runtime: &Runtime, path: &ManagedPath) -> Result<Value, FsError> {
    let field = require_field(path)?;
    let content = match path.entity {
        ManagedEntity::Agent => {
            let profile = runtime.get_or_create_agent_profile(&path.id).await;
            read_agent_field(&profile, field)?
        }
        ManagedEntity::User => {
            let profile = runtime.get_or_create_user_profile(&path.id).await;
            read_user_field(&profile, field)?
        }
    };

    Ok(json!({
        "content": content,
        "bytes": content.len()
    }))
}

pub(crate) async fn write(
    runtime: &Runtime,
    path: &ManagedPath,
    content: &str,
    allow_override: bool,
) -> Result<Value, FsError> {
    let field = require_field(path)?;

    let overwritten = match path.entity {
        ManagedEntity::Agent => {
            let mut profile = runtime.get_or_create_agent_profile(&path.id).await;
            let current = read_agent_field(&profile, field)?;
            if !allow_override && !current.is_empty() {
                return Err(FsError::already_exists(format!(
                    "managed field `{field}` already contains content"
                )));
            }
            write_agent_field(&mut profile, field, content)?;
            profile.spec_version = 0;
            profile.updated_at_unix_ms = 0;
            runtime
                .upsert_agent_profile(profile)
                .await
                .map_err(map_status)?;
            !current.is_empty()
        }
        ManagedEntity::User => {
            let mut profile = runtime.get_or_create_user_profile(&path.id).await;
            let current = read_user_field(&profile, field)?;
            if !allow_override && !current.is_empty() {
                return Err(FsError::already_exists(format!(
                    "managed field `{field}` already contains content"
                )));
            }
            write_user_field(&mut profile, field, content)?;
            profile.updated_at_unix_ms = 0;
            runtime
                .upsert_user_profile(profile)
                .await
                .map_err(map_status)?;
            !current.is_empty()
        }
    };

    Ok(json!({
        "bytes_written": content.len(),
        "created": !overwritten,
        "overwritten": overwritten
    }))
}

pub(crate) async fn replace(
    runtime: &Runtime,
    path: &ManagedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
) -> Result<Value, FsError> {
    if old.is_empty() {
        return Err(FsError::invalid_args("replace.old must be non-empty"));
    }

    let field = require_field(path)?;
    let (updated_content, replacements) = match path.entity {
        ManagedEntity::Agent => {
            let mut profile = runtime.get_or_create_agent_profile(&path.id).await;
            let current = read_agent_field(&profile, field)?;
            let (updated, replacements) = apply_replace(current, old, new, mode);
            write_agent_field(&mut profile, field, &updated)?;
            profile.spec_version = 0;
            profile.updated_at_unix_ms = 0;
            runtime
                .upsert_agent_profile(profile)
                .await
                .map_err(map_status)?;
            (updated, replacements)
        }
        ManagedEntity::User => {
            let mut profile = runtime.get_or_create_user_profile(&path.id).await;
            let current = read_user_field(&profile, field)?;
            let (updated, replacements) = apply_replace(current, old, new, mode);
            write_user_field(&mut profile, field, &updated)?;
            profile.updated_at_unix_ms = 0;
            runtime
                .upsert_user_profile(profile)
                .await
                .map_err(map_status)?;
            (updated, replacements)
        }
    };

    Ok(json!({
        "replacements": replacements,
        "bytes": updated_content.len()
    }))
}

fn require_field(path: &ManagedPath) -> Result<&str, FsError> {
    let Some(field) = path.field.as_deref() else {
        return Err(FsError::not_file(
            "managed entity root is a directory; choose a concrete field path",
        ));
    };
    validate_field(path.entity, field)?;
    Ok(field)
}

fn validate_field(entity: ManagedEntity, field: &str) -> Result<(), FsError> {
    if allowed_fields(entity).contains(&field) {
        Ok(())
    } else {
        Err(FsError::invalid_path(format!(
            "field `{field}` is not supported for {} profiles",
            match entity {
                ManagedEntity::Agent => "agent",
                ManagedEntity::User => "user",
            }
        )))
    }
}

fn allowed_fields(entity: ManagedEntity) -> &'static [&'static str] {
    match entity {
        ManagedEntity::Agent => AGENT_FIELDS,
        ManagedEntity::User => USER_FIELDS,
    }
}

fn read_agent_field(profile: &crate::pb::AgentProfile, field: &str) -> Result<String, FsError> {
    match field {
        "agents_md" => Ok(profile.agents_md.clone()),
        "soul_md" => Ok(profile.soul_md.clone()),
        "identity_md" => Ok(profile.identity_md.clone()),
        "guidelines_md" => Ok(profile.guidelines_md.clone()),
        "code_of_conduct_md" => Ok(profile.code_of_conduct_md.clone()),
        "long_term_memory_md" => Ok(profile.long_term_memory_md.clone()),
        _ => Err(FsError::invalid_path(format!(
            "unknown agent field `{field}`"
        ))),
    }
}

fn write_agent_field(
    profile: &mut crate::pb::AgentProfile,
    field: &str,
    content: &str,
) -> Result<(), FsError> {
    match field {
        "agents_md" => profile.agents_md = content.to_string(),
        "soul_md" => profile.soul_md = content.to_string(),
        "identity_md" => profile.identity_md = content.to_string(),
        "guidelines_md" => profile.guidelines_md = content.to_string(),
        "code_of_conduct_md" => profile.code_of_conduct_md = content.to_string(),
        "long_term_memory_md" => profile.long_term_memory_md = content.to_string(),
        _ => {
            return Err(FsError::invalid_path(format!(
                "unknown agent field `{field}`"
            )));
        }
    }
    Ok(())
}

fn read_user_field(profile: &crate::pb::UserProfile, field: &str) -> Result<String, FsError> {
    match field {
        "user_md" => Ok(profile.user_md.clone()),
        "preferences_json" => Ok(profile.preferences_json.clone()),
        "long_term_memory_md" => Ok(profile.long_term_memory_md.clone()),
        "name" => Ok(profile.name.clone()),
        "nickname" => Ok(profile.nickname.clone()),
        _ => Err(FsError::invalid_path(format!(
            "unknown user field `{field}`"
        ))),
    }
}

fn write_user_field(
    profile: &mut crate::pb::UserProfile,
    field: &str,
    content: &str,
) -> Result<(), FsError> {
    match field {
        "user_md" => profile.user_md = content.to_string(),
        "preferences_json" => profile.preferences_json = content.to_string(),
        "long_term_memory_md" => profile.long_term_memory_md = content.to_string(),
        "name" => profile.name = content.to_string(),
        "nickname" => profile.nickname = content.to_string(),
        _ => {
            return Err(FsError::invalid_path(format!(
                "unknown user field `{field}`"
            )));
        }
    }
    Ok(())
}

fn apply_replace(current: String, old: &str, new: &str, mode: ReplaceMode) -> (String, usize) {
    match mode {
        ReplaceMode::All => {
            let replacements = current.matches(old).count();
            let updated = current.replace(old, new);
            (updated, replacements)
        }
        ReplaceMode::First => {
            let Some(start) = current.find(old) else {
                return (current, 0);
            };
            let mut updated = String::with_capacity(current.len() - old.len() + new.len());
            updated.push_str(&current[..start]);
            updated.push_str(new);
            updated.push_str(&current[start + old.len()..]);
            (updated, 1)
        }
    }
}

fn map_status(status: tonic::Status) -> FsError {
    match status.code() {
        Code::InvalidArgument => FsError::invalid_args(status.message().to_string()),
        Code::NotFound => FsError::not_found(status.message().to_string()),
        Code::PermissionDenied => FsError::permission_denied(status.message().to_string()),
        _ => FsError::io_error(status.message().to_string()),
    }
}
