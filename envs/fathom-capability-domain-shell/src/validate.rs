use std::path::Path;

use serde_json::Value;

pub type ArgsObject = serde_json::Map<String, Value>;

pub fn args_object(args: &Value) -> Result<&ArgsObject, String> {
    args.as_object()
        .ok_or_else(|| "action arguments must be a JSON object".to_string())
}

pub fn require_non_empty_string<'a>(args: &'a ArgsObject, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing or invalid string field `{key}`"))
}

pub fn optional_non_empty_string<'a>(
    args: &'a ArgsObject,
    key: &str,
) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        Some(value) => {
            let Some(raw) = value.as_str() else {
                return Err(format!("invalid string field `{key}`"));
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(format!("invalid string field `{key}`"));
            }
            Ok(Some(trimmed))
        }
        None => Ok(None),
    }
}

pub fn optional_object<'a>(
    args: &'a ArgsObject,
    key: &str,
) -> Result<Option<&'a ArgsObject>, String> {
    match args.get(key) {
        Some(value) => value
            .as_object()
            .map(Some)
            .ok_or_else(|| format!("invalid object field `{key}`")),
        None => Ok(None),
    }
}

pub fn validate_relative_path(key: &str, value: &str) -> Result<(), String> {
    if value.contains("://") {
        return Err(format!(
            "`{key}` must be a relative filesystem path without URI scheme (received `{value}`)"
        ));
    }

    if value.starts_with('/') || value.starts_with('\\') || Path::new(value).is_absolute() {
        return Err(format!(
            "`{key}` must be relative to the capability-domain base_path (received `{value}`)"
        ));
    }

    Ok(())
}
