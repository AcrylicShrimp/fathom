use serde_json::{Value, json};

use crate::agent::ToolRegistry;
use crate::policy::system_policy;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;

pub(crate) fn build_context_payload(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    include_tools: bool,
) -> Value {
    let policy = system_policy();
    let mut payload = json!({
        "runtime_version": env!("CARGO_PKG_VERSION"),
        "workspace_root": runtime.workspace_root().display().to_string(),
        "path_policy": policy.path_policy,
        "session_identity": {
            "session_id": context.session_id.clone(),
            "active_agent_id": context.active_agent_id.clone(),
            "participant_user_ids": context.participant_user_ids.clone(),
            "active_agent_spec_version": context.active_agent_spec_version,
            "participant_user_updated_at": context.participant_user_updated_at.clone(),
        },
        "history_policy": policy.history_policy,
        "tool_policy": policy.tool_policy,
    });

    if include_tools {
        payload["tool_policy"]["known_tools"] = json!(ToolRegistry::known_tool_names());
    }

    payload
}
