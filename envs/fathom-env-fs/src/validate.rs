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

pub fn require_string<'a>(args: &'a ArgsObject, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or invalid string field `{key}`"))
}

pub fn require_boolean(args: &ArgsObject, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("missing or invalid boolean field `{key}`"))
}

pub fn require_relative_path(args: &ArgsObject, key: &str) -> Result<(), String> {
    let value = require_non_empty_string(args, key)?;

    if value.contains("://") {
        return Err(format!(
            "`{key}` must be a relative filesystem path without URI scheme (received `{value}`)"
        ));
    }

    if value.starts_with('/') || value.starts_with('\\') || Path::new(value).is_absolute() {
        return Err(format!(
            "`{key}` must be relative to the filesystem base path (received `{value}`)"
        ));
    }

    Ok(())
}
