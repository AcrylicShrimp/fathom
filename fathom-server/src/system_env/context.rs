use serde_json::{Value, json};

use crate::environment::EnvironmentRegistry;
use crate::policy::synthesize_policy_snapshot;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;

pub(crate) fn build_context_payload(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    include_actions: bool,
) -> Value {
    let policy = synthesize_policy_snapshot(include_actions);
    let time_context = runtime.current_system_time_context();
    json!({
        "runtime_version": env!("CARGO_PKG_VERSION"),
        "time_context": time_context,
        "path_policy": policy.path_policy,
        "activated_environments": EnvironmentRegistry::activated_environment_summaries(
            &context.engaged_environment_ids
        ),
        "session_identity": {
            "session_id": context.session_id.clone(),
            "active_agent_id": context.active_agent_id.clone(),
            "participant_user_ids": context.participant_user_ids.clone(),
            "active_agent_spec_version": context.active_agent_spec_version,
            "participant_user_updated_at": context.participant_user_updated_at.clone(),
            "engaged_environment_ids": context.engaged_environment_ids.clone(),
        },
        "history_policy": policy.history_policy,
        "action_policy": policy.action_policy,
    })
}

pub(crate) fn build_time_payload(runtime: &Runtime) -> Value {
    json!(runtime.current_system_time_context())
}
