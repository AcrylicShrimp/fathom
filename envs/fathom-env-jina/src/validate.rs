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

pub(crate) fn validate_http_url(field_name: &str, value: &str) -> Result<(), String> {
    let parsed =
        reqwest::Url::parse(value).map_err(|_| format!("{field_name} must be a valid URL"))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{field_name} must use http or https scheme"));
    }

    if parsed.host_str().is_none() {
        return Err(format!("{field_name} must be an absolute URL"));
    }

    Ok(())
}

pub(crate) fn optional_non_empty_string<'a>(
    args: &'a ArgsObject,
    key: &str,
) -> Result<Option<&'a str>, String> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("field `{key}` must be omitted or a non-empty string"))?;
    Ok(Some(value))
}

pub(crate) fn optional_boolean(args: &ArgsObject, key: &str) -> Result<Option<bool>, String> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_bool()
        .ok_or_else(|| format!("missing or invalid boolean field `{key}`"))?;
    Ok(Some(value))
}

pub(crate) fn optional_u64(args: &ArgsObject, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_u64()
        .ok_or_else(|| format!("missing or invalid integer field `{key}`"))?;
    Ok(Some(value))
}
