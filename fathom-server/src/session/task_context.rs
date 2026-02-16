use std::collections::BTreeMap;

use crate::session::state::SessionState;

#[derive(Debug, Clone)]
pub(crate) struct TaskExecutionContext {
    pub(crate) session_id: String,
    pub(crate) active_agent_id: String,
    pub(crate) participant_user_ids: Vec<String>,
    pub(crate) active_agent_spec_version: u64,
    pub(crate) participant_user_updated_at: BTreeMap<String, i64>,
}

impl TaskExecutionContext {
    pub(crate) fn from_state(state: &SessionState) -> Self {
        let participant_user_updated_at = state
            .participant_user_ids
            .iter()
            .map(|user_id| {
                let updated_at = state
                    .participant_user_profiles_copy
                    .get(user_id)
                    .map(|profile| profile.updated_at_unix_ms)
                    .unwrap_or_default();
                (user_id.clone(), updated_at)
            })
            .collect::<BTreeMap<_, _>>();

        Self {
            session_id: state.session_id.clone(),
            active_agent_id: state.agent_id.clone(),
            participant_user_ids: state.participant_user_ids.clone(),
            active_agent_spec_version: state.agent_profile_copy.spec_version,
            participant_user_updated_at,
        }
    }
}
