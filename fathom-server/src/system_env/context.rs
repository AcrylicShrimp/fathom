use serde_json::{Value, json};

use crate::environment::EnvironmentRegistry;
use crate::policy::system_policy;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;

pub(crate) fn build_context_payload(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    include_actions: bool,
) -> Value {
    let policy = system_policy();
    let time_context = runtime.current_system_time_context();
    let mut action_policy = policy.action_policy;
    if !include_actions {
        action_policy.known_actions.clear();
    }
    let mut payload = json!({
        "runtime_version": env!("CARGO_PKG_VERSION"),
        "workspace_root": runtime.workspace_root().display().to_string(),
        "time_context": time_context,
        "path_policy": policy.path_policy,
        "session_identity": {
            "session_id": context.session_id.clone(),
            "active_agent_id": context.active_agent_id.clone(),
            "participant_user_ids": context.participant_user_ids.clone(),
            "active_agent_spec_version": context.active_agent_spec_version,
            "participant_user_updated_at": context.participant_user_updated_at.clone(),
            "engaged_environment_ids": context.engaged_environment_ids.clone(),
        },
        "history_policy": policy.history_policy,
        "action_policy": action_policy,
        "environment_policy": policy.environment_policy,
    });

    if include_actions {
        payload["action_policy"]["known_actions"] = json!(EnvironmentRegistry::known_action_ids());
    }

    payload
}

pub(crate) fn build_time_payload(runtime: &Runtime) -> Value {
    json!(runtime.current_system_time_context())
}
