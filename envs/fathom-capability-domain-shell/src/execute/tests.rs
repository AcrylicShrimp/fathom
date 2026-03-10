use serde_json::{Value, json};

use super::execute_action;

#[cfg(unix)]
#[tokio::test]
async fn shell_run_echo_succeeds() {
    let root = unique_temp_dir("fathom-shell-echo");
    std::fs::create_dir_all(&root).expect("create temp root");

    let args = json!({
        "command": "printf 'hello'",
    });
    let outcome = execute_action(
        "run",
        args.to_string().as_str(),
        &json!({ "base_path": root.display().to_string() }),
        20_000,
    )
    .await
    .expect("shell__run should dispatch");
    assert!(outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["data"]["stdout"], json!("hello"));
    assert_eq!(payload["data"]["exit_code"], json!(0));
    assert_eq!(payload["data"]["timed_out"], json!(false));

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn shell_run_non_zero_exit_fails() {
    let root = unique_temp_dir("fathom-shell-nonzero");
    std::fs::create_dir_all(&root).expect("create temp root");

    let args = json!({
        "command": "echo 'nope' 1>&2; exit 7",
    });
    let outcome = execute_action(
        "run",
        args.to_string().as_str(),
        &json!({ "base_path": root.display().to_string() }),
        20_000,
    )
    .await
    .expect("shell__run should dispatch");
    assert!(!outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error"]["code"], json!("execution_failed"));
    assert_eq!(payload["data"]["exit_code"], json!(7));

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn shell_run_timeout_fails() {
    let root = unique_temp_dir("fathom-shell-timeout");
    std::fs::create_dir_all(&root).expect("create temp root");

    let args = json!({
        "command": "sleep 1",
    });
    let outcome = execute_action(
        "run",
        args.to_string().as_str(),
        &json!({ "base_path": root.display().to_string() }),
        10,
    )
    .await
    .expect("shell__run should dispatch");
    assert!(!outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error"]["code"], json!("timeout"));
    assert_eq!(payload["data"]["timed_out"], json!(true));

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn shell_run_rejects_escape_path() {
    let root = unique_temp_dir("fathom-shell-escape");
    std::fs::create_dir_all(&root).expect("create temp root");

    let args = json!({
        "command": "pwd",
        "path": "../../",
    });
    let outcome = execute_action(
        "run",
        args.to_string().as_str(),
        &json!({ "base_path": root.display().to_string() }),
        20_000,
    )
    .await
    .expect("shell__run should dispatch");
    assert!(!outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    let code = payload["error"]["code"].as_str().unwrap_or_default();
    assert!(!code.is_empty());

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn shell_run_applies_env_overrides() {
    let root = unique_temp_dir("fathom-shell-env");
    std::fs::create_dir_all(&root).expect("create temp root");

    let args = json!({
        "command": "printf '%s' \"$FATHOM_TEST_VAR\"",
        "env": {
            "FATHOM_TEST_VAR": "value-from-env"
        }
    });
    let outcome = execute_action(
        "run",
        args.to_string().as_str(),
        &json!({ "base_path": root.display().to_string() }),
        20_000,
    )
    .await
    .expect("shell__run should dispatch");
    assert!(outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["data"]["stdout"], json!("value-from-env"));

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn shell_run_truncates_large_stdout() {
    let root = unique_temp_dir("fathom-shell-truncate");
    std::fs::create_dir_all(&root).expect("create temp root");

    let args = json!({
        "command": "i=0; while [ $i -lt 70000 ]; do printf a; i=$((i+1)); done",
    });
    let outcome = execute_action(
        "run",
        args.to_string().as_str(),
        &json!({ "base_path": root.display().to_string() }),
        20_000,
    )
    .await
    .expect("shell__run should dispatch");
    assert!(outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    let stdout = payload["data"]["stdout"].as_str().unwrap_or_default();
    let truncated = payload["data"]["stdout_truncated_bytes"]
        .as_u64()
        .unwrap_or_default();
    assert!(!stdout.is_empty());
    assert!(truncated > 0);

    let _ = std::fs::remove_dir_all(&root);
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}
