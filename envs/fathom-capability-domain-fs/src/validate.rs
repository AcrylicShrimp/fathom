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

pub fn optional_boolean(args: &ArgsObject, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("invalid boolean field `{key}`")),
        None => Ok(None),
    }
}

pub fn optional_u64(args: &ArgsObject, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("invalid integer field `{key}`")),
        None => Ok(None),
    }
}

pub fn optional_string<'a>(args: &'a ArgsObject, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("invalid string field `{key}`")),
        None => Ok(None),
    }
}

pub fn optional_non_empty_string<'a>(
    args: &'a ArgsObject,
    key: &str,
) -> Result<Option<&'a str>, String> {
    match optional_string(args, key)? {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(format!("invalid string field `{key}`"));
            }
            Ok(Some(trimmed))
        }
        None => Ok(None),
    }
}

pub fn optional_non_empty_string_list(
    args: &ArgsObject,
    key: &str,
) -> Result<Option<Vec<String>>, String> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let list = value
        .as_array()
        .ok_or_else(|| format!("invalid string array field `{key}`"))?;
    let mut values = Vec::with_capacity(list.len());
    for (index, item) in list.iter().enumerate() {
        let Some(raw) = item.as_str() else {
            return Err(format!(
                "invalid string array field `{key}` at index {index}"
            ));
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(format!(
                "invalid string array field `{key}` at index {index}"
            ));
        }
        values.push(trimmed.to_string());
    }

    Ok(Some(values))
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
