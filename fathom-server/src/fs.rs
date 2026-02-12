mod error;
mod managed;
mod path;
mod real;
mod result;

use serde::Deserialize;

use crate::runtime::Runtime;

use self::error::FsError;
use self::path::{ParsedPath, parse_path};
pub(crate) use self::result::TaskOutcome;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReplaceMode {
    First,
    All,
}

#[derive(Debug, Deserialize)]
struct ListArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct ReadArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
    allow_override: bool,
}

#[derive(Debug, Deserialize)]
struct ReplaceArgs {
    path: String,
    old: String,
    new: String,
    mode: ReplaceMode,
}

pub(crate) async fn execute_tool(
    runtime: &Runtime,
    tool_name: &str,
    args_json: &str,
) -> Option<TaskOutcome> {
    match tool_name {
        "fs_list" => Some(execute_list(runtime, args_json).await),
        "fs_read" => Some(execute_read(runtime, args_json).await),
        "fs_write" => Some(execute_write(runtime, args_json).await),
        "fs_replace" => Some(execute_replace(runtime, args_json).await),
        _ => None,
    }
}

async fn execute_list(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    let args = match parse_args::<ListArgs>(args_json, "fs_list") {
        Ok(args) => args,
        Err(error) => return result::failure("list", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("list", Some(&args.path), &error, None),
    };
    execute_list_on_path(runtime, parsed).await
}

async fn execute_read(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    let args = match parse_args::<ReadArgs>(args_json, "fs_read") {
        Ok(args) => args,
        Err(error) => return result::failure("read", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("read", Some(&args.path), &error, None),
    };
    execute_read_on_path(runtime, parsed).await
}

async fn execute_write(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    let args = match parse_args::<WriteArgs>(args_json, "fs_write") {
        Ok(args) => args,
        Err(error) => return result::failure("write", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("write", Some(&args.path), &error, None),
    };
    execute_write_on_path(runtime, parsed, &args.content, args.allow_override).await
}

async fn execute_replace(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    let args = match parse_args::<ReplaceArgs>(args_json, "fs_replace") {
        Ok(args) => args,
        Err(error) => return result::failure("replace", None, &error, None),
    };

    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("replace", Some(&args.path), &error, None),
    };
    execute_replace_on_path(runtime, parsed, &args.old, &args.new, args.mode).await
}

async fn execute_list_on_path(runtime: &Runtime, path: ParsedPath) -> TaskOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_uri().to_string();

    let result = match path {
        ParsedPath::Managed(path) => managed::list(runtime, &path).await,
        ParsedPath::Real(path) => real::list(runtime, &path),
    };

    match result {
        Ok(data) => result::success("list", &normalized_path, target, data),
        Err(error) => result::failure("list", Some(&normalized_path), &error, Some(target)),
    }
}

async fn execute_read_on_path(runtime: &Runtime, path: ParsedPath) -> TaskOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_uri().to_string();

    let result = match path {
        ParsedPath::Managed(path) => managed::read(runtime, &path).await,
        ParsedPath::Real(path) => real::read(runtime, &path),
    };

    match result {
        Ok(data) => result::success("read", &normalized_path, target, data),
        Err(error) => result::failure("read", Some(&normalized_path), &error, Some(target)),
    }
}

async fn execute_write_on_path(
    runtime: &Runtime,
    path: ParsedPath,
    content: &str,
    allow_override: bool,
) -> TaskOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_uri().to_string();

    let result = match path {
        ParsedPath::Managed(path) => managed::write(runtime, &path, content, allow_override).await,
        ParsedPath::Real(path) => real::write(runtime, &path, content, allow_override),
    };

    match result {
        Ok(data) => result::success("write", &normalized_path, target, data),
        Err(error) => result::failure("write", Some(&normalized_path), &error, Some(target)),
    }
}

async fn execute_replace_on_path(
    runtime: &Runtime,
    path: ParsedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
) -> TaskOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_uri().to_string();

    let result = match path {
        ParsedPath::Managed(path) => managed::replace(runtime, &path, old, new, mode).await,
        ParsedPath::Real(path) => real::replace(runtime, &path, old, new, mode),
    };

    match result {
        Ok(data) => result::success("replace", &normalized_path, target, data),
        Err(error) => result::failure("replace", Some(&normalized_path), &error, Some(target)),
    }
}

fn parse_args<T>(args_json: &str, tool_name: &str) -> Result<T, FsError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(args_json).map_err(|error| {
        FsError::invalid_args(format!("failed to parse args for `{tool_name}`: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use crate::runtime::Runtime;

    use super::execute_tool;

    #[tokio::test]
    async fn fs_tools_write_and_read_managed_agent_field() {
        let runtime = Runtime::new(2, 10);
        let write_outcome = execute_tool(
            &runtime,
            "fs_write",
            r#"{"path":"managed://agent/agent-a/long_term_memory_md","content":"hello memory","allow_override":true}"#,
        )
        .await
        .expect("fs_write should dispatch");
        assert!(write_outcome.succeeded);

        let read_outcome = execute_tool(
            &runtime,
            "fs_read",
            r#"{"path":"managed://agent/agent-a/long_term_memory_md"}"#,
        )
        .await
        .expect("fs_read should dispatch");
        assert!(read_outcome.succeeded);

        let payload: Value =
            serde_json::from_str(&read_outcome.message).expect("valid json payload");
        let content = payload
            .get("data")
            .and_then(|data| data.get("content"))
            .and_then(Value::as_str)
            .expect("content field must exist");
        assert_eq!(content, "hello memory");
    }

    #[tokio::test]
    async fn fs_tools_replace_supports_mode_switch() {
        let root = unique_temp_dir("fathom-fs-replace");
        std::fs::create_dir_all(&root).expect("create temp root");
        let runtime = Runtime::new_with_workspace_root(2, 10, root.clone()).expect("runtime");

        let write_outcome = execute_tool(
            &runtime,
            "fs_write",
            r#"{"path":"fs://notes.txt","content":"a a a","allow_override":true}"#,
        )
        .await
        .expect("fs_write should dispatch");
        assert!(write_outcome.succeeded);

        let replace_first = execute_tool(
            &runtime,
            "fs_replace",
            r#"{"path":"fs://notes.txt","old":"a","new":"z","mode":"first"}"#,
        )
        .await
        .expect("fs_replace first should dispatch");
        assert!(replace_first.succeeded);

        let read_after_first = execute_tool(&runtime, "fs_read", r#"{"path":"fs://notes.txt"}"#)
            .await
            .expect("fs_read should dispatch");
        let payload_first: Value =
            serde_json::from_str(&read_after_first.message).expect("valid json payload");
        assert_eq!(
            payload_first["data"]["content"]
                .as_str()
                .unwrap_or_default(),
            "z a a"
        );

        let replace_all = execute_tool(
            &runtime,
            "fs_replace",
            r#"{"path":"fs://notes.txt","old":"a","new":"x","mode":"all"}"#,
        )
        .await
        .expect("fs_replace all should dispatch");
        assert!(replace_all.succeeded);

        let read_after_all = execute_tool(&runtime, "fs_read", r#"{"path":"fs://notes.txt"}"#)
            .await
            .expect("fs_read should dispatch");
        let payload_all: Value =
            serde_json::from_str(&read_after_all.message).expect("valid json payload");
        assert_eq!(
            payload_all["data"]["content"].as_str().unwrap_or_default(),
            "z x x"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn fs_tools_reject_workspace_escape() {
        let root = unique_temp_dir("fathom-fs-escape");
        std::fs::create_dir_all(&root).expect("create temp root");
        let runtime = Runtime::new_with_workspace_root(2, 10, root.clone()).expect("runtime");

        let outcome = execute_tool(&runtime, "fs_read", r#"{"path":"fs://../../etc/passwd"}"#)
            .await
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

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }
}
