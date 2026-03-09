use super::Runtime;
use crate::agent::{
    ActionModeSupportContract, AgentInvocationContext, CapabilityAction, CapabilityEnvironment,
    CapabilityRecipe, CapabilitySurface, HarnessContract, IdentityEnvelope, ParticipantEnvelope,
    ResolvedPayloadLookupHint, SessionAnchor, SessionBaseline,
};
use crate::environment::EnvironmentRegistry;
use crate::profile_material::{agent_identity_material, participant_profile_material};
use crate::session::SessionState;
use fathom_env::ActionModeSupport;
use fathom_protocol::pb;
use serde_json::json;

impl Runtime {
    pub(crate) fn build_agent_invocation_context(
        &self,
        state: &SessionState,
        triggers: &[pb::Trigger],
    ) -> AgentInvocationContext {
        const HISTORY_WINDOW_SIZE: usize = 80;
        let recent_history = if state.history.len() > HISTORY_WINDOW_SIZE {
            state.history[state.history.len() - HISTORY_WINDOW_SIZE..].to_vec()
        } else {
            state.history.clone()
        };

        let resolved_payload_lookups = state
            .pending_payload_lookups
            .iter()
            .map(|lookup| ResolvedPayloadLookupHint {
                lookup_execution_id: lookup.lookup_execution_id.clone(),
                execution_id: lookup.execution_id.clone(),
                part: lookup.part.clone(),
                offset: lookup.offset,
                next_offset: lookup.next_offset,
                full_bytes: lookup.full_bytes,
                source_truncated: lookup.source_truncated,
                payload_chunk: lookup.payload_chunk.clone(),
                injected_truncated: lookup.injected_truncated,
                injected_omitted_bytes: lookup.injected_omitted_bytes,
            })
            .collect::<Vec<_>>();

        AgentInvocationContext {
            harness_contract: self.build_harness_contract(),
            identity_envelope: self.build_identity_envelope(state),
            session_baseline: self.build_session_baseline(state),
            resolved_payload_lookups,
            triggers: triggers.to_vec(),
            recent_history,
            compaction: state.compaction.clone(),
        }
    }

    fn build_harness_contract(&self) -> HarnessContract {
        HarnessContract {
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            contract_schema_version: 1,
        }
    }

    fn build_identity_envelope(&self, state: &SessionState) -> IdentityEnvelope {
        IdentityEnvelope {
            schema_version: 1,
            source_revision: format!(
                "{}@spec:{}@updated:{}",
                &state.agent_profile_copy.agent_id,
                state.agent_profile_copy.spec_version,
                state.agent_profile_copy.updated_at_unix_ms
            ),
            material: agent_identity_material(&state.agent_profile_copy),
        }
    }

    fn build_session_baseline(&self, state: &SessionState) -> SessionBaseline {
        SessionBaseline {
            session_anchor: SessionAnchor {
                session_id: state.session_id.clone(),
                started_at_unix_ms: state.created_at_unix_ms,
            },
            capability_surface: self.build_capability_surface(state),
            participant_envelope: self.build_participant_envelope(state),
        }
    }

    fn build_capability_surface(&self, state: &SessionState) -> CapabilitySurface {
        let mut environments = state
            .engaged_environment_ids
            .iter()
            .filter_map(|environment_id| {
                let environment = EnvironmentRegistry::environment_summary(environment_id)?;
                let mut actions =
                    EnvironmentRegistry::environment_action_summaries(environment_id)?
                        .into_iter()
                        .map(|action| CapabilityAction {
                            action_id: action.id,
                            description: action.description,
                            mode_support: map_mode_support(action.mode_support),
                            discovery: action.discovery,
                        })
                        .collect::<Vec<_>>();
                actions.sort_by(|a, b| a.action_id.cmp(&b.action_id));
                let mut recipes = environment
                    .recipes
                    .into_iter()
                    .map(|recipe| CapabilityRecipe {
                        title: recipe.title,
                        steps: recipe.steps,
                    })
                    .collect::<Vec<_>>();
                recipes.sort_by(|a, b| a.title.cmp(&b.title));
                Some(CapabilityEnvironment {
                    id: environment.id,
                    name: environment.name,
                    description: environment.description,
                    actions,
                    recipes,
                })
            })
            .collect::<Vec<_>>();
        environments.sort_by(|a, b| a.id.cmp(&b.id));
        CapabilitySurface { environments }
    }

    fn build_participant_envelope(&self, state: &SessionState) -> ParticipantEnvelope {
        let participants = state
            .participant_user_ids
            .iter()
            .filter_map(|user_id| state.participant_user_profiles_copy.get(user_id))
            .map(participant_profile_material)
            .collect::<Vec<_>>();
        ParticipantEnvelope {
            schema_version: 1,
            source_revision: participant_envelope_source_revision(state),
            material: json!({
                "participants": participants,
            }),
        }
    }
}

fn participant_envelope_source_revision(state: &SessionState) -> String {
    state
        .participant_user_ids
        .iter()
        .map(|user_id| {
            let updated_at = state
                .participant_user_profiles_copy
                .get(user_id)
                .map(|profile| profile.updated_at_unix_ms)
                .unwrap_or_default();
            format!("{user_id}@{updated_at}")
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn map_mode_support(mode_support: ActionModeSupport) -> ActionModeSupportContract {
    match mode_support {
        ActionModeSupport::AwaitOnly => ActionModeSupportContract::AwaitOnly,
        ActionModeSupport::AwaitOrDetach => ActionModeSupportContract::AwaitOrDetach,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::Runtime;
    use crate::agent::{ActionModeSupportContract, SessionCompaction, SummaryBlockRef};
    use crate::environment::EnvironmentRegistry;
    use crate::session::SessionState;
    use crate::util::{default_agent_profile, default_user_profile};
    use serde_json::json;

    #[test]
    fn agent_invocation_context_builds_stable_prefix_layers() {
        let runtime = Runtime::new(2, 10);
        let user_id = "user-a".to_string();
        let state = SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            EnvironmentRegistry::default_engaged_environment_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            EnvironmentRegistry::initial_environment_snapshots()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        );

        let context = runtime.build_agent_invocation_context(&state, &[]);
        assert_eq!(context.harness_contract.contract_schema_version, 1);
        assert_eq!(
            context.identity_envelope.source_revision,
            format!(
                "agent-a@spec:{}@updated:{}",
                state.agent_profile_copy.spec_version, state.agent_profile_copy.updated_at_unix_ms
            )
        );
        assert_eq!(
            context.session_baseline.session_anchor.started_at_unix_ms,
            state.created_at_unix_ms
        );
        assert_eq!(
            context
                .session_baseline
                .participant_envelope
                .source_revision,
            format!(
                "user-a@{}",
                state
                    .participant_user_profiles_copy
                    .get("user-a")
                    .expect("participant profile")
                    .updated_at_unix_ms
            )
        );
    }

    #[test]
    fn agent_invocation_context_includes_capability_surface_with_mode_support() {
        let runtime = Runtime::new(2, 10);
        let user_id = "user-a".to_string();
        let state = SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            EnvironmentRegistry::default_engaged_environment_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            EnvironmentRegistry::initial_environment_snapshots()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        );

        let context = runtime.build_agent_invocation_context(&state, &[]);
        assert!(
            !context
                .session_baseline
                .capability_surface
                .environments
                .is_empty()
        );
        assert!(
            context
                .session_baseline
                .capability_surface
                .environments
                .iter()
                .all(|environment| !environment.actions.is_empty())
        );
        assert!(
            context
                .session_baseline
                .capability_surface
                .environments
                .iter()
                .flat_map(|environment| environment.actions.iter())
                .any(|action| action.mode_support == ActionModeSupportContract::AwaitOrDetach)
        );
        assert!(
            context
                .session_baseline
                .capability_surface
                .environments
                .iter()
                .flat_map(|environment| environment.actions.iter())
                .all(|action| matches!(
                    action.mode_support,
                    ActionModeSupportContract::AwaitOnly | ActionModeSupportContract::AwaitOrDetach
                ))
        );
    }

    #[test]
    fn agent_invocation_context_rebuilds_stable_prefix_from_authoritative_state_even_with_compaction()
     {
        let runtime = Runtime::new(2, 10);
        let user_id = "user-a".to_string();
        let mut state = SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            EnvironmentRegistry::default_engaged_environment_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            EnvironmentRegistry::initial_environment_snapshots()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        );

        state.agent_profile_copy.display_name = "Updated Agent".to_string();
        state
            .participant_user_profiles_copy
            .get_mut("user-a")
            .expect("participant profile")
            .name = "Updated User".to_string();
        state.engaged_environment_ids =
            BTreeSet::from(["filesystem".to_string(), "shell".to_string()]);
        state.compaction = SessionCompaction {
            last_compacted_history_index: 24,
            summary_blocks: vec![SummaryBlockRef {
                id: "history-summary-000024".to_string(),
                source_range_start: 0,
                source_range_end: 24,
                summary_text: "history-summary-000024 source=[0,24) execution_requested=9 awaited_execution_succeeded=9 actions=[system__get_time] users=[user-stale]".to_string(),
                created_at_unix_ms: 1_765_000_000_000,
            }],
        };

        let context = runtime.build_agent_invocation_context(&state, &[]);

        assert_eq!(
            context.identity_envelope.material["display_name"],
            json!("Updated Agent")
        );
        assert_eq!(
            context.session_baseline.participant_envelope.material["participants"][0]["name"],
            json!("Updated User")
        );
        let environment_ids = context
            .session_baseline
            .capability_surface
            .environments
            .iter()
            .map(|environment| environment.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(environment_ids, vec!["filesystem", "shell"]);
    }
}
