use serde_json::{Value, json};

use super::execute_action;

#[tokio::test]
async fn jina_read_url_rejects_invalid_url() {
    let args = json!({
        "url": "file:///tmp/nope"
    });
    let outcome = execute_action("read_url", args.to_string().as_str(), &json!({}), 10_000)
        .await
        .expect("jina__read_url should dispatch");
    assert!(!outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error"]["code"], json!("invalid_args"));
}

#[tokio::test]
async fn jina_read_url_requires_auth_key() {
    let args = json!({
        "url": "https://example.com"
    });

    if std::env::var("JINA_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .is_some()
    {
        return;
    }

    let outcome = execute_action("read_url", args.to_string().as_str(), &json!({}), 10_000)
        .await
        .expect("jina__read_url should dispatch");
    assert!(!outcome.succeeded);

    let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
    assert_eq!(payload["error"]["code"], json!("auth_missing"));
}
