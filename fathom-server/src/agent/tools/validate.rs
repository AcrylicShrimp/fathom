use serde_json::Value;

type ArgsObject = serde_json::Map<String, Value>;

pub(crate) fn validate_tool_args(tool_name: &str, args: &Value) -> Result<(), String> {
    let args_obj = args
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;

    match tool_name {
        "memory_append" => {
            let target = require_enum(args_obj, "target", &["agent", "user"])?;
            if target != "agent" && target != "user" {
                return Err("memory_append.target must be 'agent' or 'user'".to_string());
            }
            require_non_empty_string(args_obj, "target_id")?;
            require_non_empty_string(args_obj, "note")?;
            Ok(())
        }
        "refresh_profile" => {
            let scope = require_enum(args_obj, "scope", &["agent", "user", "all"])?;
            if scope != "agent" && scope != "user" && scope != "all" {
                return Err("refresh_profile.scope must be 'agent', 'user', or 'all'".to_string());
            }
            if scope == "user" {
                require_non_empty_string(args_obj, "user_id")?;
            }
            Ok(())
        }
        "send_message" => {
            require_non_empty_string(args_obj, "content")?;
            require_optional_string(args_obj, "user_id", "send_message.user_id must be a string")?;
            Ok(())
        }
        "fs_list" | "fs_read" => {
            require_managed_or_fs_path(args_obj, "path")?;
            Ok(())
        }
        "fs_write" => {
            require_managed_or_fs_path(args_obj, "path")?;
            require_string(args_obj, "content", "fs_write.content must be a string")?;
            require_bool(
                args_obj,
                "allow_override",
                "fs_write.allow_override must be a boolean",
            )?;
            Ok(())
        }
        "fs_replace" => {
            require_managed_or_fs_path(args_obj, "path")?;
            require_non_empty_string(args_obj, "old")?;
            require_string(args_obj, "new", "fs_replace.new must be a string")?;
            let mode = require_non_empty_string(args_obj, "mode")?;
            if mode != "first" && mode != "all" {
                return Err("fs_replace.mode must be `first` or `all`".to_string());
            }
            Ok(())
        }
        "sys_get_context" => {
            if let Some(include_tools) = args_obj.get("include_tools")
                && !include_tools.is_boolean()
            {
                return Err("sys_get_context.include_tools must be a boolean".to_string());
            }
            Ok(())
        }
        "sys_get_time" => {
            if !args_obj.is_empty() {
                return Err("sys_get_time does not accept arguments".to_string());
            }
            Ok(())
        }
        "sys_list_profiles" => {
            let kind = require_non_empty_string(args_obj, "kind")?;
            if kind != "agent" && kind != "user" && kind != "all" {
                return Err("sys_list_profiles.kind must be `agent`, `user`, or `all`".to_string());
            }
            Ok(())
        }
        "sys_get_session_identity_map" => Ok(()),
        "sys_get_profile" => {
            let kind = require_non_empty_string(args_obj, "kind")?;
            if kind != "agent" && kind != "user" {
                return Err("sys_get_profile.kind must be `agent` or `user`".to_string());
            }
            require_non_empty_string(args_obj, "id")?;
            let view = require_non_empty_string(args_obj, "view")?;
            if view != "summary" && view != "full" {
                return Err("sys_get_profile.view must be `summary` or `full`".to_string());
            }
            Ok(())
        }
        "sys_get_task_payload" => {
            require_non_empty_string(args_obj, "task_id")?;
            let part = require_non_empty_string(args_obj, "part")?;
            if part != "args" && part != "result" {
                return Err("sys_get_task_payload.part must be `args` or `result`".to_string());
            }
            require_optional_u64(
                args_obj,
                "offset",
                "sys_get_task_payload.offset must be a non-negative integer",
            )?;
            require_optional_u64(
                args_obj,
                "limit",
                "sys_get_task_payload.limit must be a non-negative integer",
            )?;
            Ok(())
        }
        _ => Err(format!("unknown tool `{tool_name}`")),
    }
}

fn require_non_empty_string<'a>(args: &'a ArgsObject, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing or invalid string field `{key}`"))
}

fn require_string<'a>(
    args: &'a ArgsObject,
    key: &str,
    error_message: &str,
) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| error_message.to_string())
}

fn require_optional_string(
    args: &ArgsObject,
    key: &str,
    error_message: &str,
) -> Result<(), String> {
    if let Some(value) = args.get(key)
        && !value.is_string()
    {
        return Err(error_message.to_string());
    }
    Ok(())
}

fn require_bool(args: &ArgsObject, key: &str, error_message: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| error_message.to_string())
}

fn require_optional_u64(args: &ArgsObject, key: &str, error_message: &str) -> Result<(), String> {
    if let Some(value) = args.get(key)
        && value.as_u64().is_none()
    {
        return Err(error_message.to_string());
    }
    Ok(())
}

fn require_enum<'a>(args: &'a ArgsObject, key: &str, _allowed: &[&str]) -> Result<&'a str, String> {
    require_non_empty_string(args, key)
}

fn require_managed_or_fs_path<'a>(args: &'a ArgsObject, key: &str) -> Result<&'a str, String> {
    let path = require_non_empty_string(args, key)?;
    if !path.starts_with("managed://") && !path.starts_with("fs://") {
        return Err("path must start with managed:// or fs://".to_string());
    }
    Ok(path)
}
