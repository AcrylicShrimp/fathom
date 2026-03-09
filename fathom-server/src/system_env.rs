mod context;
mod environments;
mod executions;
mod profiles;

use serde::Deserialize;
use serde_json::json;

use crate::runtime::Runtime;
use crate::session::execution_context::ExecutionContext;
use fathom_env::ActionOutcome;

use self::executions::parse_execution_payload_part;
use self::profiles::{parse_profile_kind, parse_profile_view};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetContextArgs {}

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
#[serde(deny_unknown_fields)]
struct DescribeEnvironmentArgs {
    env_id: String,
}

#[derive(Debug, Deserialize)]
struct GetExecutionPayloadArgs {
    execution_id: String,
    part: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: usize,
}

pub(crate) async fn execute_action(
    runtime: &Runtime,
    context: &ExecutionContext,
    action_name: &str,
    args_json: &str,
) -> Option<ActionOutcome> {
    match action_name {
        "get_context" => Some(execute_get_context(runtime, context, args_json).await),
        "get_time" => Some(execute_get_time(runtime, args_json)),
        "list_profiles" => Some(execute_list_profiles(runtime, args_json).await),
        "describe_environment" => Some(execute_describe_environment(context, args_json)),
        "get_session_identity_map" => Some(execute_get_session_identity_map(context)),
        "get_profile" => Some(execute_get_profile(runtime, args_json).await),
        "get_execution_payload" => {
            Some(execute_get_execution_payload(runtime, context, args_json).await)
        }
        _ => None,
    }
}

async fn execute_get_context(
    runtime: &Runtime,
    context: &ExecutionContext,
    args_json: &str,
) -> ActionOutcome {
    match parse_args::<GetContextArgs>(args_json, "system__get_context") {
        Ok(_) => {}
        Err(error) => return failure("system__get_context", error),
    }

    success(
        "system__get_context",
        context::build_context_payload(runtime, context),
    )
}

fn execute_get_time(runtime: &Runtime, args_json: &str) -> ActionOutcome {
    if let Err(error) = parse_args::<GetTimeArgs>(args_json, "system__get_time") {
        return failure("system__get_time", error);
    }
    success("system__get_time", context::build_time_payload(runtime))
}

async fn execute_list_profiles(runtime: &Runtime, args_json: &str) -> ActionOutcome {
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

fn execute_describe_environment(context: &ExecutionContext, args_json: &str) -> ActionOutcome {
    let args =
        match parse_args::<DescribeEnvironmentArgs>(args_json, "system__describe_environment") {
            Ok(args) => args,
            Err(error) => return failure("system__describe_environment", error),
        };
    let env_id = args.env_id.trim();
    if env_id.is_empty() {
        return failure(
            "system__describe_environment",
            "env_id must be non-empty".to_string(),
        );
    }
    if !context
        .engaged_environment_ids
        .iter()
        .any(|id| id == env_id)
    {
        return failure(
            "system__describe_environment",
            format!("environment `{env_id}` is not activated in this session"),
        );
    }

    match environments::describe_environment(env_id) {
        Some(payload) => success("system__describe_environment", payload),
        None => failure(
            "system__describe_environment",
            format!("unknown environment `{env_id}`"),
        ),
    }
}

fn execute_get_session_identity_map(context: &ExecutionContext) -> ActionOutcome {
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

async fn execute_get_profile(runtime: &Runtime, args_json: &str) -> ActionOutcome {
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

async fn execute_get_execution_payload(
    runtime: &Runtime,
    context: &ExecutionContext,
    args_json: &str,
) -> ActionOutcome {
    let args =
        match parse_args::<GetExecutionPayloadArgs>(args_json, "system__get_execution_payload") {
            Ok(args) => args,
            Err(error) => return failure("system__get_execution_payload", error),
        };
    if args.execution_id.trim().is_empty() {
        return failure(
            "system__get_execution_payload",
            "execution_id must be non-empty".to_string(),
        );
    }

    let part = match parse_execution_payload_part(&args.part) {
        Ok(part) => part,
        Err(error) => return failure("system__get_execution_payload", error),
    };

    match executions::get_execution_payload(
        runtime,
        context,
        &args.execution_id,
        part,
        args.offset,
        args.limit,
    )
    .await
    {
        Ok(payload) => success("system__get_execution_payload", payload),
        Err(error) => failure("system__get_execution_payload", error),
    }
}

fn parse_args<T>(args_json: &str, action_id: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(args_json)
        .map_err(|error| format!("failed to parse args for `{action_id}`: {error}"))
}

fn success(op: &str, data: serde_json::Value) -> ActionOutcome {
    ActionOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": op,
            "data": data,
        })
        .to_string(),
        state_patch: None,
    }
}

fn failure(op: &str, message: String) -> ActionOutcome {
    ActionOutcome {
        succeeded: false,
        message: json!({
            "ok": false,
            "op": op,
            "error_code": "invalid_args",
            "message": message,
        })
        .to_string(),
        state_patch: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::runtime::Runtime;
    use crate::session::execution_context::ExecutionContext;

    use super::execute_action;

    #[tokio::test]
    async fn system_get_context_returns_payload() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(&runtime, &context, "get_context", "{}")
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
    async fn system_get_context_includes_environment_summaries() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(&runtime, &context, "get_context", "{}")
            .await
            .expect("should dispatch get_context");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let environments = payload["data"]["activated_environments"]
            .as_array()
            .expect("activated_environments must be an array");

        assert!(!environments.is_empty());
    }

    #[tokio::test]
    async fn system_get_context_excludes_policy_fields() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
        };

        let outcome = execute_action(&runtime, &context, "get_context", "{}")
            .await
            .expect("should dispatch get_context");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let data = payload.get("data").expect("data field must exist");

        assert!(data.get("path_policy").is_none());
        assert!(data.get("history_policy").is_none());
        assert!(data.get("action_policy").is_none());
        assert!(data.get("workspace_root").is_none());
        assert!(data["activated_environments"].is_array());
    }

    #[tokio::test]
    async fn system_describe_environment_requires_activation() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
            session_id: "session-1".to_string(),
            active_agent_id: "agent-1".to_string(),
            participant_user_ids: vec!["user-1".to_string()],
            active_agent_spec_version: 1,
            participant_user_updated_at: [("user-1".to_string(), 123)].into_iter().collect(),
            engaged_environment_ids: vec!["system".to_string()],
        };

        let outcome = execute_action(
            &runtime,
            &context,
            "describe_environment",
            r#"{"env_id":"filesystem"}"#,
        )
        .await
        .expect("should dispatch describe_environment");
        assert!(!outcome.succeeded);
    }

    #[tokio::test]
    async fn system_describe_environment_returns_action_inventory() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
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
            "describe_environment",
            r#"{"env_id":"filesystem"}"#,
        )
        .await
        .expect("should dispatch describe_environment");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        assert_eq!(payload["data"]["id"], serde_json::json!("filesystem"));
        let actions = payload["data"]["actions"]
            .as_array()
            .expect("actions must be an array");
        assert!(
            actions
                .iter()
                .any(|action| { action["id"] == serde_json::json!("filesystem__get_base_path") })
        );
    }

    #[tokio::test]
    async fn system_describe_environment_includes_filesystem_path_rules() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
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
            "describe_environment",
            r#"{"env_id":"filesystem"}"#,
        )
        .await
        .expect("should dispatch describe_environment");
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        let capabilities = payload["data"]["capabilities"]
            .as_array()
            .expect("capabilities must be an array");
        assert!(capabilities.iter().any(|capability| {
            capability
                .as_str()
                .is_some_and(|value| value.contains("use `.` to target root"))
        }));

        let recipes = payload["data"]["recipes"]
            .as_array()
            .expect("recipes must be an array");
        let flattened_steps = recipes
            .iter()
            .filter_map(|recipe| recipe["steps"].as_array())
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert!(
            flattened_steps
                .iter()
                .any(|step| step.contains("Do not use empty path"))
        );
        assert!(flattened_steps.iter().any(|step| step.contains("path '.'")));
    }

    #[tokio::test]
    async fn system_get_time_returns_canonical_time_context() {
        let runtime = Runtime::new(2, 10);
        let context = ExecutionContext {
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
        let context = ExecutionContext {
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
