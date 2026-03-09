use fathom_env::{ActionModeSupport, ActionSpec};
use serde_json::Value;

pub(super) const SYSTEM_ENVIRONMENT_ID: &str = "system";
const SYSTEM_ACTION_MAX_TIMEOUT_MS: u64 = 5_000;
const SYSTEM_ACTION_DESIRED_TIMEOUT_MS: u64 = 2_000;

pub(super) type ArgsObject = serde_json::Map<String, Value>;

pub(super) fn system_spec(
    action_name: &'static str,
    description: &'static str,
    input_schema: Value,
) -> ActionSpec {
    ActionSpec {
        environment_id: SYSTEM_ENVIRONMENT_ID,
        action_name,
        description,
        input_schema,
        discovery: true,
        mode_support: ActionModeSupport::AwaitOnly,
        max_timeout_ms: SYSTEM_ACTION_MAX_TIMEOUT_MS,
        desired_timeout_ms: Some(SYSTEM_ACTION_DESIRED_TIMEOUT_MS),
    }
}

pub(super) fn args_object(args: &Value) -> Result<&ArgsObject, String> {
    args.as_object()
        .ok_or_else(|| "action arguments must be a JSON object".to_string())
}

pub(super) fn require_non_empty_string<'a>(
    args: &'a ArgsObject,
    key: &str,
) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing or invalid string field `{key}`"))
}

pub(super) fn require_optional_u64(
    args: &ArgsObject,
    key: &str,
    error_message: &str,
) -> Result<(), String> {
    if let Some(value) = args.get(key)
        && value.as_u64().is_none()
    {
        return Err(error_message.to_string());
    }
    Ok(())
}
