use std::collections::BTreeMap;

use super::Runtime;
use crate::agent::{SessionIdentityMapSnapshot, SystemContextSnapshot, TurnSnapshot};
use crate::pb;
use crate::policy::system_policy;
use crate::session::SessionState;

impl Runtime {
    pub(crate) fn build_turn_snapshot(
        &self,
        state: &SessionState,
        turn_id: u64,
        triggers: &[pb::Trigger],
    ) -> TurnSnapshot {
        const HISTORY_WINDOW_SIZE: usize = 80;
        let recent_history = if state.history.len() > HISTORY_WINDOW_SIZE {
            state.history[state.history.len() - HISTORY_WINDOW_SIZE..].to_vec()
        } else {
            state.history.clone()
        };

        let participant_profiles = state
            .participant_user_ids
            .iter()
            .filter_map(|id| state.participant_user_profiles_copy.get(id).cloned())
            .collect::<Vec<_>>();

        TurnSnapshot {
            session_id: state.session_id.clone(),
            turn_id,
            system_context: self.build_system_context_snapshot(state),
            agent_profile: state.agent_profile_copy.clone(),
            participant_profiles,
            triggers: triggers.to_vec(),
            recent_history,
            compaction: state.compaction.clone(),
        }
    }

    fn build_system_context_snapshot(&self, state: &SessionState) -> SystemContextSnapshot {
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

        let policy = system_policy();

        SystemContextSnapshot {
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            workspace_root: self.workspace_root().display().to_string(),
            time_context: self.current_system_time_context(),
            path_policy: policy.path_policy,
            session_identity: SessionIdentityMapSnapshot {
                session_id: state.session_id.clone(),
                active_agent_id: state.agent_id.clone(),
                participant_user_ids: state.participant_user_ids.clone(),
                active_agent_spec_version: state.agent_profile_copy.spec_version,
                participant_user_updated_at,
            },
            tool_policy: policy.tool_policy,
            history_policy: policy.history_policy,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::Runtime;
    use crate::session::SessionState;
    use crate::util::{default_agent_profile, default_user_profile};

    #[test]
    fn turn_snapshot_includes_time_context() {
        let runtime = Runtime::new(2, 10);
        let user_id = "user-a".to_string();
        let state = SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
        );

        let snapshot = runtime.build_turn_snapshot(&state, 1, &[]);
        let time_context = snapshot.system_context.time_context;

        assert!(!time_context.utc_rfc3339.trim().is_empty());
        assert!(!time_context.local_rfc3339.trim().is_empty());
        assert!(!time_context.local_timezone_name.trim().is_empty());
        assert!(!time_context.local_utc_offset.trim().is_empty());
        assert_eq!(time_context.time_source, "server_clock");
    }
}
