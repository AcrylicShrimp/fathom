use serde_json::{Value, json};

use crate::capability_domain::CapabilityDomainRegistry;
use crate::runtime::Runtime;
use crate::session::execution_context::ExecutionContext;

pub(crate) fn build_context_payload(runtime: &Runtime, context: &ExecutionContext) -> Value {
    let time_context = runtime.current_system_time_context();
    json!({
        "runtime_version": env!("CARGO_PKG_VERSION"),
        "time_context": time_context,
        "activated_capability_domains": CapabilityDomainRegistry::activated_capability_domain_summaries(
            &context.engaged_capability_domain_ids
        ),
        "session_identity": {
            "session_id": context.session_id.clone(),
            "active_agent_id": context.active_agent_id.clone(),
            "participant_user_ids": context.participant_user_ids.clone(),
            "active_agent_spec_version": context.active_agent_spec_version,
            "participant_user_updated_at": context.participant_user_updated_at.clone(),
            "engaged_capability_domain_ids": context.engaged_capability_domain_ids.clone(),
        },
    })
}

pub(crate) fn build_time_payload(runtime: &Runtime) -> Value {
    json!(runtime.current_system_time_context())
}
