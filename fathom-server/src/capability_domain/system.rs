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
            description: "Privileged runtime and identity inspection capability domain.",
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
                title: "Refresh authoritative session context".to_string(),
                steps: vec![
                    "Call system__get_context to fetch runtime version, current server time, activated capability domains, and session identity map.".to_string(),
                    "Use this before planning multi-step action sequences when context may have changed.".to_string(),
                    "Call system__get_time when you need fresher wall-clock data mid-turn.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Expand execution preview into full payload".to_string(),
                steps: vec![
                    "Start from execution_requested and execution outcome previews in history to identify the relevant execution_id.".to_string(),
                    "Call system__get_execution_payload with {execution_id, part} to load full args/result content.".to_string(),
                    "Use offset/limit to page large payloads instead of requesting everything at once.".to_string(),
                    "After inspecting payloads, continue planning with concrete failure/success evidence.".to_string(),
                ],
            },
        ]
    }
}
