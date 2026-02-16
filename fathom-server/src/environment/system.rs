mod common;
mod describe_environment;
mod get_context;
mod get_profile;
mod get_session_identity_map;
mod get_task_payload;
mod get_time;
mod list_profiles;

use std::sync::Arc;

use fathom_env::{Action, Environment, EnvironmentSpec};
use serde_json::Value;

use common::SYSTEM_ENVIRONMENT_ID;
use describe_environment::DescribeEnvironmentAction;
use get_context::GetContextAction;
use get_profile::GetProfileAction;
use get_session_identity_map::GetSessionIdentityMapAction;
use get_task_payload::GetTaskPayloadAction;
use get_time::GetTimeAction;
use list_profiles::ListProfilesAction;

pub(super) struct SystemEnvironment;

impl Environment for SystemEnvironment {
    fn spec(&self) -> EnvironmentSpec {
        EnvironmentSpec {
            id: SYSTEM_ENVIRONMENT_ID,
            name: "System",
            description: "Privileged runtime and identity inspection environment.",
        }
    }

    fn initial_state(&self) -> Value {
        serde_json::json!({})
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(DescribeEnvironmentAction),
            Arc::new(GetContextAction),
            Arc::new(GetTimeAction),
            Arc::new(ListProfilesAction),
            Arc::new(GetSessionIdentityMapAction),
            Arc::new(GetProfileAction),
            Arc::new(GetTaskPayloadAction),
        ]
    }
}
