mod common;
mod describe_capability_domain;
mod get_context;
mod get_execution_payload;
mod get_profile;
mod get_session_identity_map;
mod get_time;
mod list_profiles;

use std::sync::Arc;

use fathom_capability_domain::{
    Action, CapabilityDomain, CapabilityDomainRecipe, CapabilityDomainSpec,
};
use serde_json::Value;

use common::SYSTEM_CAPABILITY_DOMAIN_ID;
use describe_capability_domain::DescribeCapabilityDomainAction;
use get_context::GetContextAction;
use get_execution_payload::GetExecutionPayloadAction;
use get_profile::GetProfileAction;
use get_session_identity_map::GetSessionIdentityMapAction;
use get_time::GetTimeAction;
use list_profiles::ListProfilesAction;

pub(super) struct SystemCapabilityDomain;

impl CapabilityDomain for SystemCapabilityDomain {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: SYSTEM_CAPABILITY_DOMAIN_ID,
            name: "System",
            description: "Privileged runtime inspection capability domain for authoritative session, time, profile, and execution-payload data.",
        }
    }

    fn initial_state(&self) -> Value {
        serde_json::json!({})
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(DescribeCapabilityDomainAction),
            Arc::new(GetContextAction),
            Arc::new(GetTimeAction),
            Arc::new(ListProfilesAction),
            Arc::new(GetSessionIdentityMapAction),
            Arc::new(GetProfileAction),
            Arc::new(GetExecutionPayloadAction),
        ]
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Refresh authoritative runtime context".to_string(),
                steps: vec![
                    "Call `system__get_context` when you need a fresh snapshot of session and runtime state.".to_string(),
                    "Use the returned server time and activation data as the authoritative baseline for the current decision.".to_string(),
                    "Call `system__get_time` later in the session when only clock freshness is needed.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Inspect execution payloads".to_string(),
                steps: vec![
                    "Start with the `execution_id` and choose whether you need the `args` or `result` payload.".to_string(),
                    "Call `system__get_execution_payload` to load the payload body for that execution.".to_string(),
                    "Use `offset` and `limit` to page large payloads instead of requesting everything at once.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Inspect runtime profiles".to_string(),
                steps: vec![
                    "Call `system__list_profiles` with the desired `kind` to discover available ids.".to_string(),
                    "Call `system__get_profile` with one id when you need summary or full profile data.".to_string(),
                    "Call `system__get_session_identity_map` when you need the active identities for the current session.".to_string(),
                ],
            },
        ]
    }
}
