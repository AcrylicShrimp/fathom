mod error;
mod path;
mod real;
mod result;

use fathom_env::ActionOutcome;
use serde::Deserialize;
use serde_json::{Value, json};

use self::error::FsError;
use self::path::{ParsedPath, parse_path, resolve_base_path};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReplaceMode {
    First,
    All,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
    allow_override: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceArgs {
    path: String,
    old: String,
    new: String,
    mode: ReplaceMode,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetBasePathArgs {}

pub fn execute_action(
    action_name: &str,
    args_json: &str,
    environment_state: &Value,
) -> Option<ActionOutcome> {
    match action_name {
        "get_base_path" => Some(execute_get_base_path(args_json, environment_state)),
        "list" => Some(execute_list(args_json, environment_state)),
        "read" => Some(execute_read(args_json, environment_state)),
        "write" => Some(execute_write(args_json, environment_state)),
        "replace" => Some(execute_replace(args_json, environment_state)),
        _ => None,
    }
}

fn execute_get_base_path(args_json: &str, environment_state: &Value) -> ActionOutcome {
    if let Err(error) = parse_args::<GetBasePathArgs>(args_json, "filesystem__get_base_path") {
        return result::failure("get_base_path", Some("."), &error, Some("filesystem"));
    }

    match resolve_base_path(environment_state) {
        Ok(base_path) => result::success(
            "get_base_path",
            ".",
            "filesystem",
            json!({
                "base_path": base_path.display().to_string(),
                "source": "filesystem_env_state"
            }),
        ),
        Err(error) => result::failure("get_base_path", Some("."), &error, Some("filesystem")),
    }
}

fn execute_list(args_json: &str, environment_state: &Value) -> ActionOutcome {
    let args = match parse_args::<ListArgs>(args_json, "filesystem__list") {
        Ok(args) => args,
        Err(error) => return result::failure("list", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("list", Some(&args.path), &error, None),
    };
    execute_list_on_path(parsed, environment_state)
}

fn execute_read(args_json: &str, environment_state: &Value) -> ActionOutcome {
    let args = match parse_args::<ReadArgs>(args_json, "filesystem__read") {
        Ok(args) => args,
        Err(error) => return result::failure("read", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("read", Some(&args.path), &error, None),
    };
    execute_read_on_path(parsed, environment_state)
}

fn execute_write(args_json: &str, environment_state: &Value) -> ActionOutcome {
    let args = match parse_args::<WriteArgs>(args_json, "filesystem__write") {
        Ok(args) => args,
        Err(error) => return result::failure("write", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("write", Some(&args.path), &error, None),
    };
    execute_write_on_path(
        parsed,
        &args.content,
        args.allow_override,
        environment_state,
    )
}

fn execute_replace(args_json: &str, environment_state: &Value) -> ActionOutcome {
    let args = match parse_args::<ReplaceArgs>(args_json, "filesystem__replace") {
        Ok(args) => args,
        Err(error) => return result::failure("replace", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("replace", Some(&args.path), &error, None),
    };
    execute_replace_on_path(parsed, &args.old, &args.new, args.mode, environment_state)
}

fn execute_list_on_path(path: ParsedPath, environment_state: &Value) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::list(&path, environment_state) {
        Ok(data) => result::success("list", &normalized_path, target, data),
        Err(error) => result::failure("list", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_read_on_path(path: ParsedPath, environment_state: &Value) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::read(&path, environment_state) {
        Ok(data) => result::success("read", &normalized_path, target, data),
        Err(error) => result::failure("read", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_write_on_path(
    path: ParsedPath,
    content: &str,
    allow_override: bool,
    environment_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::write(&path, content, allow_override, environment_state) {
        Ok(data) => result::success("write", &normalized_path, target, data),
        Err(error) => result::failure("write", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_replace_on_path(
    path: ParsedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
    environment_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::replace(&path, old, new, mode, environment_state) {
        Ok(data) => result::success("replace", &normalized_path, target, data),
        Err(error) => result::failure("replace", Some(&normalized_path), &error, Some(target)),
    }
}

fn parse_args<T>(args_json: &str, action_id: &str) -> Result<T, FsError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(args_json).map_err(|error| {
        FsError::invalid_args(format!("failed to parse args for `{action_id}`: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::execute_action;

    #[test]
    fn fs_env_replace_supports_mode_switch() {
        let root = unique_temp_dir("fathom-fs-replace");
        std::fs::create_dir_all(&root).expect("create temp root");

        let write_outcome = execute_action(
            "write",
            r#"{"path":"notes.txt","content":"a a a","allow_override":true}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_write should dispatch");
        assert!(write_outcome.succeeded);

        let replace_first = execute_action(
            "replace",
            r#"{"path":"notes.txt","old":"a","new":"z","mode":"first"}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_replace first should dispatch");
        assert!(replace_first.succeeded);

        let read_after_first = execute_action(
            "read",
            r#"{"path":"notes.txt"}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_read should dispatch");
        let payload_first: Value =
            serde_json::from_str(&read_after_first.message).expect("valid json payload");
        assert_eq!(
            payload_first["data"]["content"]
                .as_str()
                .unwrap_or_default(),
            "z a a"
        );

        let replace_all = execute_action(
            "replace",
            r#"{"path":"notes.txt","old":"a","new":"x","mode":"all"}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_replace all should dispatch");
        assert!(replace_all.succeeded);

        let read_after_all = execute_action(
            "read",
            r#"{"path":"notes.txt"}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_read should dispatch");
        let payload_all: Value =
            serde_json::from_str(&read_after_all.message).expect("valid json payload");
        assert_eq!(
            payload_all["data"]["content"].as_str().unwrap_or_default(),
            "z x x"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fs_env_reject_workspace_escape() {
        let root = unique_temp_dir("fathom-fs-escape");
        std::fs::create_dir_all(&root).expect("create temp root");

        let outcome = execute_action(
            "read",
            r#"{"path":"../../etc/passwd"}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_read should dispatch");
        assert!(!outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let code = payload
            .get("error_code")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(!code.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fs_env_reject_absolute_path() {
        let root = unique_temp_dir("fathom-fs-absolute");
        std::fs::create_dir_all(&root).expect("create temp root");

        let outcome = execute_action(
            "read",
            r#"{"path":"/etc/passwd"}"#,
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("fs_read should dispatch");
        assert!(!outcome.succeeded);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fs_env_get_base_path_returns_canonical_scope() {
        let root = unique_temp_dir("fathom-fs-base-path");
        std::fs::create_dir_all(&root).expect("create temp root");

        let outcome = execute_action(
            "get_base_path",
            "{}",
            &json!({ "base_path": root.display().to_string() }),
        )
        .expect("filesystem__get_base_path should dispatch");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let canonical_root =
            std::fs::canonicalize(&root).expect("base path should canonicalize for comparison");
        assert_eq!(payload["data"]["source"], json!("filesystem_env_state"));
        assert_eq!(
            payload["data"]["base_path"].as_str().unwrap_or_default(),
            canonical_root.display().to_string()
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }
}
