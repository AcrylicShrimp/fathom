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
