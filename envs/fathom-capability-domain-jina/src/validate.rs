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
