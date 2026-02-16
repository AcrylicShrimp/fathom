mod context;
mod profiles;
mod tasks;

use serde::Deserialize;
use serde_json::json;

use crate::fs::TaskOutcome;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;

use self::profiles::{parse_profile_kind, parse_profile_view};
use self::tasks::parse_task_payload_part;

#[derive(Debug, Deserialize)]
struct GetContextArgs {
    #[serde(default)]
    include_actions: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetTimeArgs {}

#[derive(Debug, Deserialize)]
struct ListProfilesArgs {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct GetProfileArgs {
    kind: String,
    id: String,
    view: String,
}

#[derive(Debug, Deserialize)]
struct GetTaskPayloadArgs {
    task_id: String,
    part: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: usize,
}

pub(crate) async fn execute_action(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    action_name: &str,
    args_json: &str,
) -> Option<TaskOutcome> {
    match action_name {
        "get_context" => Some(execute_get_context(runtime, context, args_json).await),
        "get_time" => Some(execute_get_time(runtime, args_json)),
        "list_profiles" => Some(execute_list_profiles(runtime, args_json).await),
        "get_session_identity_map" => Some(execute_get_session_identity_map(context)),
        "get_profile" => Some(execute_get_profile(runtime, args_json).await),
        "get_task_payload" => Some(execute_get_task_payload(runtime, context, args_json).await),
        _ => None,
    }
}

async fn execute_get_context(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    args_json: &str,
) -> TaskOutcome {
    let args = match parse_args::<GetContextArgs>(args_json, "system__get_context") {
        Ok(args) => args,
        Err(error) => return failure("system__get_context", error),
    };

    success(
        "system__get_context",
        context::build_context_payload(runtime, context, args.include_actions),
    )
}

fn execute_get_time(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    if let Err(error) = parse_args::<GetTimeArgs>(args_json, "system__get_time") {
        return failure("system__get_time", error);
    }
    success("system__get_time", context::build_time_payload(runtime))
}

async fn execute_list_profiles(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    let args = match parse_args::<ListProfilesArgs>(args_json, "system__list_profiles") {
        Ok(args) => args,
        Err(error) => return failure("system__list_profiles", error),
    };

    let kind = match parse_profile_kind(&args.kind) {
        Ok(kind) => kind,
        Err(error) => return failure("system__list_profiles", error),
    };

    success(
        "system__list_profiles",
        profiles::list_profiles(runtime, kind).await,
    )
}

fn execute_get_session_identity_map(context: &TaskExecutionContext) -> TaskOutcome {
    success(
        "system__get_session_identity_map",
        json!({
            "session_id": context.session_id.clone(),
            "active_agent_id": context.active_agent_id.clone(),
            "participant_user_ids": context.participant_user_ids.clone(),
            "active_agent_spec_version": context.active_agent_spec_version,
            "participant_user_updated_at": context.participant_user_updated_at.clone(),
            "engaged_environment_ids": context.engaged_environment_ids.clone(),
        }),
    )
}

async fn execute_get_profile(runtime: &Runtime, args_json: &str) -> TaskOutcome {
    let args = match parse_args::<GetProfileArgs>(args_json, "system__get_profile") {
        Ok(args) => args,
        Err(error) => return failure("system__get_profile", error),
    };
    if args.id.trim().is_empty() {
        return failure("system__get_profile", "id must be non-empty".to_string());
    }

    let kind = match parse_profile_kind(&args.kind) {
        Ok(kind) => kind,
        Err(error) => return failure("system__get_profile", error),
    };
    let view = match parse_profile_view(&args.view) {
        Ok(view) => view,
        Err(error) => return failure("system__get_profile", error),
    };

    match profiles::get_profile(runtime, kind, &args.id, view).await {
        Ok(payload) => success("system__get_profile", payload),
        Err(error) => failure("system__get_profile", error),
    }
}

async fn execute_get_task_payload(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    args_json: &str,
) -> TaskOutcome {
    let args = match parse_args::<GetTaskPayloadArgs>(args_json, "system__get_task_payload") {
        Ok(args) => args,
        Err(error) => return failure("system__get_task_payload", error),
    };
    if args.task_id.trim().is_empty() {
        return failure(
            "system__get_task_payload",
            "task_id must be non-empty".to_string(),
        );
    }

    let part = match parse_task_payload_part(&args.part) {
        Ok(part) => part,
        Err(error) => return failure("system__get_task_payload", error),
    };

    match tasks::get_task_payload(
        runtime,
        context,
        &args.task_id,
        part,
        args.offset,
        args.limit,
    )
    .await
    {
        Ok(payload) => success("system__get_task_payload", payload),
        Err(error) => failure("system__get_task_payload", error),
    }
}

fn parse_args<T>(args_json: &str, action_id: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(args_json)
        .map_err(|error| format!("failed to parse args for `{action_id}`: {error}"))
}

fn success(op: &str, data: serde_json::Value) -> TaskOutcome {
    TaskOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "data": data,
        })
        .to_string(),
    }
}

fn failure(op: &str, message: String) -> TaskOutcome {
    TaskOutcome {
        succeeded: false,
        message: json!({
            "ok": false,
            "op": op,
            "error_code": "invalid_args",
            "message": message,
        })
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::environment::EnvironmentRegistry;
    use crate::policy::system_policy;
    use crate::runtime::Runtime;
    use crate::session::task_context::TaskExecutionContext;

    use super::execute_action;

    #[tokio::test]
    async fn system_get_context_returns_payload() {
        let runtime = Runtime::new(2, 10);
        let context = TaskExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(
            &runtime,
            &context,
            "get_context",
            r#"{"include_actions":true}"#,
        )
        .await
        .expect("should dispatch get_context");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let time_context = payload["data"]["time_context"]
            .as_object()
            .expect("time_context must be an object");
        assert!(time_context.contains_key("utc_rfc3339"));
        assert!(time_context.contains_key("local_rfc3339"));
        assert!(time_context.contains_key("local_timezone_name"));
    }

    #[tokio::test]
    async fn system_get_context_includes_known_actions_from_registry() {
        let runtime = Runtime::new(2, 10);
        let context = TaskExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(
            &runtime,
            &context,
            "get_context",
            r#"{"include_actions":true}"#,
        )
        .await
        .expect("should dispatch get_context");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let known_actions = payload["data"]["action_policy"]["known_actions"]
            .as_array()
            .expect("known_actions must be an array")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(known_actions, EnvironmentRegistry::known_action_ids());
    }

    #[tokio::test]
    async fn system_get_context_uses_canonical_policy_snapshot() {
        let runtime = Runtime::new(2, 10);
        let context = TaskExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };
        let policy = system_policy();

        let outcome = execute_action(
            &runtime,
            &context,
            "get_context",
            r#"{"include_actions":true}"#,
        )
        .await
        .expect("should dispatch get_context");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let data = payload.get("data").expect("data field must exist");

        assert_eq!(
            data["path_policy"]["managed_uri_patterns"],
            serde_json::json!(policy.path_policy.managed_uri_patterns)
        );
        assert_eq!(
            data["path_policy"]["fs_uri_policy"],
            serde_json::json!(policy.path_policy.fs_uri_policy)
        );
        assert_eq!(
            data["history_policy"]["lookup_action"],
            serde_json::json!(policy.history_policy.lookup_action)
        );
        assert_eq!(
            data["action_policy"]["known_actions"],
            serde_json::json!(policy.action_policy.known_actions)
        );
    }

    #[tokio::test]
    async fn system_get_time_returns_canonical_time_context() {
        let runtime = Runtime::new(2, 10);
        let context = TaskExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(&runtime, &context, "get_time", "{}")
            .await
            .expect("should dispatch get_time");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let data = payload.get("data").expect("data field must exist");
        assert!(data["utc_rfc3339"].as_str().is_some());
        assert!(data["local_rfc3339"].as_str().is_some());
        assert!(data["local_timezone_name"].as_str().is_some());
        assert_eq!(data["time_source"].as_str(), Some("server_clock"));
    }

    #[tokio::test]
    async fn system_get_time_rejects_unknown_fields() {
        let runtime = Runtime::new(2, 10);
        let context = TaskExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(&runtime, &context, "get_time", r#"{"unexpected":1}"#)
            .await
            .expect("should dispatch get_time");
        assert!(!outcome.succeeded);
    }
}
