use serde_json::{Map, Value};

pub(crate) type ArgsObject = Map<String, Value>;

pub(crate) fn args_object(args: &Value) -> Result<&ArgsObject, String> {
    args.as_object()
        .ok_or_else(|| "action arguments must be a JSON object".to_string())
}

pub(crate) fn require_non_empty_string<'a>(
    args: &'a ArgsObject,
    key: &str,
) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing or invalid string field `{key}`"))
}

pub(crate) fn optional_u64(args: &ArgsObject, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("missing or invalid integer field `{key}`")),
        None => Ok(None),
    }
}
