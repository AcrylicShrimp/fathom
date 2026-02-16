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

#[test]
fn fs_env_read_supports_line_windowing() {
    let root = unique_temp_dir("fathom-fs-read-window");
    std::fs::create_dir_all(&root).expect("create temp root");
    std::fs::write(root.join("note.txt"), "a\nb\nc\nd\n").expect("write file");

    let outcome = execute_action(
        "read",
        r#"{"path":"note.txt","offset_line":2,"limit_lines":2}"#,
        &json!({ "base_path": root.display().to_string() }),
    )
    .expect("filesystem__read should dispatch");
    assert!(outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["data"]["content"], json!("b\nc"));
    assert_eq!(payload["data"]["start_line"], json!(2));
    assert_eq!(payload["data"]["returned_lines"], json!(2));
    assert_eq!(payload["data"]["total_lines"], json!(4));
    assert_eq!(payload["data"]["truncated"], json!(true));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fs_env_read_rejects_non_utf8_file() {
    let root = unique_temp_dir("fathom-fs-read-non-utf8");
    std::fs::create_dir_all(&root).expect("create temp root");
    std::fs::write(root.join("bin.dat"), [0xffu8, 0xfdu8]).expect("write non utf8");

    let outcome = execute_action(
        "read",
        r#"{"path":"bin.dat"}"#,
        &json!({ "base_path": root.display().to_string() }),
    )
    .expect("filesystem__read should dispatch");
    assert!(!outcome.succeeded);
    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error_code"], json!("invalid_encoding"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fs_env_glob_returns_matching_files() {
    let root = unique_temp_dir("fathom-fs-glob");
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    std::fs::write(root.join("src").join("main.rs"), "fn main() {}\n").expect("write main");
    std::fs::write(root.join("src").join("lib.rs"), "pub fn lib() {}\n").expect("write lib");
    std::fs::write(root.join("src").join("note.txt"), "note\n").expect("write note");

    let outcome = execute_action(
        "glob",
        r#"{"path":"src","pattern":"**/*.rs"}"#,
        &json!({ "base_path": root.display().to_string() }),
    )
    .expect("filesystem__glob should dispatch");
    assert!(outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    let matches = payload["data"]["matches"]
        .as_array()
        .expect("matches must be array");
    assert!(matches.iter().any(|value| value == "src/main.rs"));
    assert!(matches.iter().any(|value| value == "src/lib.rs"));
    assert!(!matches.iter().any(|value| value == "src/note.txt"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fs_env_search_uses_regex_and_case_insensitive_default() {
    let root = unique_temp_dir("fathom-fs-search");
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    std::fs::write(root.join("src").join("main.rs"), "fn Main() {\n}\n").expect("write main");
    std::fs::write(root.join("src").join("lib.rs"), "pub fn helper() {}\n").expect("write lib");

    let outcome = execute_action(
        "search",
        r#"{"path":"src","pattern":"fn\\s+main\\(","include":["**/*.rs"]}"#,
        &json!({ "base_path": root.display().to_string() }),
    )
    .expect("filesystem__search should dispatch");
    assert!(outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    let matches = payload["data"]["matches"]
        .as_array()
        .expect("matches must be array");
    assert!(
        matches
            .iter()
            .any(|value| value["path"] == json!("src/main.rs"))
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fs_env_replace_enforces_expected_replacements() {
    let root = unique_temp_dir("fathom-fs-replace-expected");
    std::fs::create_dir_all(&root).expect("create temp root");
    std::fs::write(root.join("target.txt"), "one one").expect("write file");

    let outcome = execute_action(
        "replace",
        r#"{"path":"target.txt","old":"one","new":"two","mode":"all","expected_replacements":3}"#,
        &json!({ "base_path": root.display().to_string() }),
    )
    .expect("filesystem__replace should dispatch");
    assert!(!outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error_code"], json!("invalid_args"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fs_env_write_respects_create_parents_flag() {
    let root = unique_temp_dir("fathom-fs-write-create-parents");
    std::fs::create_dir_all(&root).expect("create temp root");

    let outcome = execute_action(
        "write",
        r#"{"path":"nested/file.txt","content":"x","allow_override":true,"create_parents":false}"#,
        &json!({ "base_path": root.display().to_string() }),
    )
    .expect("filesystem__write should dispatch");
    assert!(!outcome.succeeded);
    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error_code"], json!("not_found"));

    let _ = std::fs::remove_dir_all(&root);
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}
