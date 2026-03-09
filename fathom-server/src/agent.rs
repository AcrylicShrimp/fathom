mod action_catalog;
mod model_adapter;
mod openai;
mod prompt;
mod prompt_assembler;
mod retry;
mod types;

pub(crate) use types::{
    ActionInvocation, ActionModeSupportSnapshot, AgentTurnOutcome, CapabilityActionSnapshot,
    CapabilityEnvironmentSnapshot, CapabilityRecipeSnapshot, CapabilitySurfaceSnapshot,
    HarnessContractSnapshot, IdentityEnvelopeSnapshot, InFlightActionHint, ModelDeltaEvent,
    ModelInvocationOutcome, ParticipantEnvelopeSnapshot, PromptMessage, PromptMessageBundle,
    ResolvedPayloadLookupHint, SessionAnchorSnapshot, SessionBaselineSnapshot,
    SessionCompactionSnapshot, StreamNote, SummaryBlockRefSnapshot, SystemTimeContext,
    TurnSnapshot,
};

use std::sync::Arc;

use crate::environment::EnvironmentRegistry;
pub(crate) use action_catalog::SessionActionCatalog;
use model_adapter::{ModelAdapter, UnavailableModelAdapter};
use openai::OpenAiModelAdapter;
use prompt_assembler::PromptAssembler;

#[derive(Clone)]
pub(crate) struct AgentOrchestrator {
    model_adapter: Arc<dyn ModelAdapter>,
    environment_registry: EnvironmentRegistry,
    prompt_assembler: PromptAssembler,
}

impl AgentOrchestrator {
    pub(crate) fn new() -> Self {
        let model_adapter: Arc<dyn ModelAdapter> = match OpenAiModelAdapter::new() {
            Ok(adapter) => Arc::new(adapter),
            Err(error) => Arc::new(UnavailableModelAdapter::new("openai", error)),
        };
        Self::from_parts(
            model_adapter,
            EnvironmentRegistry::new(),
            PromptAssembler::new(),
        )
    }

    pub(crate) fn assemble_prompt_bundle(
        &self,
        snapshot: &TurnSnapshot,
        retry_feedback: Option<&str>,
    ) -> PromptMessageBundle {
        self.prompt_assembler.assemble(snapshot, retry_feedback)
    }

    fn session_action_catalog(&self, snapshot: &TurnSnapshot) -> SessionActionCatalog {
        SessionActionCatalog::from_snapshot(self.environment_registry.clone(), snapshot)
    }

    fn from_parts(
        model_adapter: Arc<dyn ModelAdapter>,
        environment_registry: EnvironmentRegistry,
        prompt_assembler: PromptAssembler,
    ) -> Self {
        Self {
            model_adapter,
            environment_registry,
            prompt_assembler,
        }
    }

    #[cfg(test)]
    fn with_model_adapter(model_adapter: Arc<dyn ModelAdapter>) -> Self {
        Self::from_parts(
            model_adapter,
            EnvironmentRegistry::new(),
            PromptAssembler::new(),
        )
    }

    pub(crate) async fn run_turn<F>(
        &self,
        snapshot: &TurnSnapshot,
        initial_prompt_bundle: PromptMessageBundle,
        mut on_event: F,
    ) -> AgentTurnOutcome
    where
        F: FnMut(ModelDeltaEvent) + Send,
    {
        if let Some(error) = self.model_adapter.availability_error() {
            return AgentTurnOutcome::failure(
                "agent_init_error",
                format!(
                    "model adapter `{}` initialization failed: {error}",
                    self.model_adapter.provider_name()
                ),
                Vec::new(),
            );
        }

        let mut diagnostics = Vec::new();
        let mut retry_feedback: Option<String> = None;
        let action_catalog = self.session_action_catalog(snapshot);

        for semantic_attempt in 0..=1usize {
            on_event(ModelDeltaEvent::StreamNote(StreamNote {
                phase: "agent.turn.attempt".to_string(),
                detail: format!("semantic_attempt={}", semantic_attempt + 1),
            }));

            let prompt_bundle = if semantic_attempt == 0 {
                initial_prompt_bundle.clone()
            } else {
                self.assemble_prompt_bundle(snapshot, retry_feedback.as_deref())
            };
            on_event(ModelDeltaEvent::StreamNote(StreamNote {
                phase: "agent.prompt.summary".to_string(),
                detail: format!(
                    "messages={} estimated_tokens={} compaction_applied={} dedup_dropped={}",
                    prompt_bundle.stats.messages_count,
                    prompt_bundle.stats.estimated_prompt_tokens,
                    prompt_bundle.stats.compaction_applied,
                    prompt_bundle.stats.dedup_dropped_events
                ),
            }));
            let event_sink: &mut model_adapter::ModelEventSink<'_> = &mut on_event;
            let result = self
                .model_adapter
                .stream_prompt(&prompt_bundle.messages, &action_catalog, event_sink)
                .await;

            match result {
                Ok(invocation_outcome)
                    if invocation_outcome.action_call_count > 0
                        || !invocation_outcome.assistant_outputs.is_empty() =>
                {
                    diagnostics.extend(invocation_outcome.diagnostics);
                    diagnostics.push(format!(
                        "prompt_messages={} estimated_tokens={} compaction_applied={} timeline_raw={} timeline_compacted={} dedup_dropped={}",
                        prompt_bundle.stats.messages_count,
                        prompt_bundle.stats.estimated_prompt_tokens,
                        prompt_bundle.stats.compaction_applied,
                        prompt_bundle.stats.timeline_raw_events,
                        prompt_bundle.stats.timeline_compacted_events,
                        prompt_bundle.stats.dedup_dropped_events
                    ));
                    diagnostics.push(format!(
                        "action_calls_dispatched={} assistant_outputs={} on attempt {}",
                        invocation_outcome.action_call_count,
                        invocation_outcome.assistant_outputs.len(),
                        semantic_attempt + 1
                    ));
                    return AgentTurnOutcome::success(
                        invocation_outcome.action_call_count,
                        invocation_outcome.assistant_outputs,
                        diagnostics,
                    );
                }
                Ok(invocation_outcome) => {
                    diagnostics.extend(invocation_outcome.diagnostics);
                    diagnostics.push(format!(
                        "prompt_messages={} estimated_tokens={} compaction_applied={} timeline_raw={} timeline_compacted={} dedup_dropped={}",
                        prompt_bundle.stats.messages_count,
                        prompt_bundle.stats.estimated_prompt_tokens,
                        prompt_bundle.stats.compaction_applied,
                        prompt_bundle.stats.timeline_raw_events,
                        prompt_bundle.stats.timeline_compacted_events,
                        prompt_bundle.stats.dedup_dropped_events
                    ));
                    diagnostics.push(format!(
                        "no action call or assistant output generated on attempt {}",
                        semantic_attempt + 1
                    ));

                    if semantic_attempt == 0 {
                        retry_feedback = Some(
                            "No valid executable action call or assistant output was produced. \
You MUST emit at least one valid action call or assistant output."
                                .to_string(),
                        );
                        continue;
                    }

                    return AgentTurnOutcome::failure(
                        "no_action_or_output",
                        "agent produced no executable action call or assistant output after retry",
                        diagnostics,
                    );
                }
                Err(error) => {
                    diagnostics.push(format!(
                        "model adapter `{}` request failed: {error}",
                        self.model_adapter.provider_name()
                    ));
                    if semantic_attempt == 0 && is_recoverable_action_error(&error) {
                        retry_feedback = Some(build_retry_feedback(&error));
                        diagnostics.push(
                            "retrying semantic attempt due to recoverable action-call error"
                                .to_string(),
                        );
                        continue;
                    }
                    return AgentTurnOutcome::failure("model_adapter_error", error, diagnostics);
                }
            }
        }

        AgentTurnOutcome::failure(
            "agent_unreachable",
            "unexpected agent loop termination",
            diagnostics,
        )
    }
}

fn is_recoverable_action_error(error: &str) -> bool {
    error.contains("validation failed")
        || error.contains("invalid arguments JSON for action")
        || error.contains("unknown action `")
        || error.contains("is not available in this session")
}

fn build_retry_feedback(error: &str) -> String {
    let mut feedback = format!(
        "The previous action call was invalid and could not be executed: {error}\n\
Emit a corrected action call with valid arguments, or emit assistant output."
    );
    if is_optional_string_validation_error(error) {
        feedback.push_str(
            "\nFor optional string fields, omit the field instead of sending empty strings.",
        );
    }
    feedback
}

fn is_optional_string_validation_error(error: &str) -> bool {
    error.contains("validation failed") && error.contains("must be omitted or a non-empty string")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::build_retry_feedback;
    use super::model_adapter::{ModelAdapter, ModelAdapterFuture, ModelEventSink};
    use super::types::PromptBuildStats;
    use super::{
        AgentOrchestrator, CapabilityEnvironmentSnapshot, CapabilitySurfaceSnapshot,
        HarnessContractSnapshot, IdentityEnvelopeSnapshot, ModelDeltaEvent, ModelInvocationOutcome,
        ParticipantEnvelopeSnapshot, PromptMessage, PromptMessageBundle, SessionAnchorSnapshot,
        SessionBaselineSnapshot, SessionCompactionSnapshot, TurnSnapshot,
    };
    use crate::util::default_agent_profile;
    use serde_json::json;

    struct FakeModelAdapter {
        availability_error: Option<String>,
        outcomes: Mutex<VecDeque<Result<ModelInvocationOutcome, String>>>,
        prompt_message_counts: Mutex<Vec<usize>>,
    }

    impl FakeModelAdapter {
        fn with_outcomes(outcomes: Vec<Result<ModelInvocationOutcome, String>>) -> Self {
            Self {
                availability_error: None,
                outcomes: Mutex::new(VecDeque::from(outcomes)),
                prompt_message_counts: Mutex::new(Vec::new()),
            }
        }

        fn unavailable(message: &str) -> Self {
            Self {
                availability_error: Some(message.to_string()),
                outcomes: Mutex::new(VecDeque::new()),
                prompt_message_counts: Mutex::new(Vec::new()),
            }
        }
    }

    impl ModelAdapter for FakeModelAdapter {
        fn provider_name(&self) -> &'static str {
            "fake"
        }

        fn availability_error(&self) -> Option<&str> {
            self.availability_error.as_deref()
        }

        fn stream_prompt<'a>(
            &'a self,
            prompt_messages: &'a [PromptMessage],
            _action_catalog: &'a super::SessionActionCatalog,
            _on_event: &'a mut ModelEventSink<'a>,
        ) -> ModelAdapterFuture<'a> {
            self.prompt_message_counts
                .lock()
                .expect("prompt counts mutex")
                .push(prompt_messages.len());
            let result = self
                .outcomes
                .lock()
                .expect("outcomes mutex")
                .pop_front()
                .expect("configured fake outcome");
            Box::pin(async move { result })
        }
    }

    fn test_snapshot() -> TurnSnapshot {
        let agent_profile = default_agent_profile("agent-default");
        TurnSnapshot {
            session_id: "session-1".to_string(),
            turn_id: 1,
            harness_contract: HarnessContractSnapshot {
                runtime_version: "0.1.0".to_string(),
                contract_schema_version: 1,
            },
            identity_envelope: IdentityEnvelopeSnapshot {
                schema_version: 1,
                source_revision: format!(
                    "{}@spec:{}@updated:{}",
                    &agent_profile.agent_id,
                    agent_profile.spec_version,
                    agent_profile.updated_at_unix_ms
                ),
                material: json!({
                    "display_name": agent_profile.display_name.clone(),
                    "soul_md": agent_profile.soul_md.clone(),
                    "identity_md": agent_profile.identity_md.clone(),
                    "agents_md": agent_profile.agents_md.clone(),
                    "guidelines_md": agent_profile.guidelines_md.clone(),
                }),
            },
            session_baseline: SessionBaselineSnapshot {
                session_anchor: SessionAnchorSnapshot {
                    session_id: "session-1".to_string(),
                    started_at_unix_ms: 1_765_000_000_000,
                },
                capability_surface: CapabilitySurfaceSnapshot {
                    environments: vec![CapabilityEnvironmentSnapshot {
                        id: "filesystem".to_string(),
                        name: "Filesystem".to_string(),
                        description: "Stateful filesystem environment rooted at a base path."
                            .to_string(),
                        actions: vec![],
                        recipes: vec![],
                    }],
                },
                participant_envelope: ParticipantEnvelopeSnapshot {
                    schema_version: 1,
                    source_revision: "user-default@1765000000000".to_string(),
                    material: json!({"participants": []}),
                },
            },
            in_flight_actions: vec![],
            resolved_payload_lookups: vec![],
            triggers: vec![],
            recent_history: vec![],
            compaction: SessionCompactionSnapshot::default(),
        }
    }

    #[test]
    fn retry_feedback_guides_optional_string_omission() {
        let feedback = build_retry_feedback(
            "action `jina__read_url` validation failed: field `remove_selector` must be omitted or a non-empty string",
        );
        assert!(feedback.contains("omit the field instead of sending empty strings"));
    }

    #[test]
    fn retry_feedback_generic_error_keeps_base_instruction() {
        let feedback = build_retry_feedback(
            "action `filesystem__read` validation failed: missing or invalid string field `path`",
        );
        assert!(feedback.contains("Emit a corrected action call with valid arguments"));
        assert!(!feedback.contains("omit the field instead of sending empty strings"));
    }

    #[tokio::test]
    async fn run_turn_retries_after_recoverable_model_adapter_error() {
        let fake_adapter = Arc::new(FakeModelAdapter::with_outcomes(vec![
            Err(
                "action `filesystem__read` validation failed: missing or invalid string field `path`"
                    .to_string(),
            ),
            Ok(ModelInvocationOutcome {
                action_call_count: 1,
                assistant_outputs: vec![],
                diagnostics: vec!["adapter success".to_string()],
            }),
        ]));
        let orchestrator = AgentOrchestrator::with_model_adapter(fake_adapter.clone());
        let snapshot = test_snapshot();
        let initial_prompt_bundle = PromptMessageBundle {
            messages: vec![PromptMessage::new(
                "user",
                "initial_turn",
                "initial prompt".to_string(),
            )],
            stats: PromptBuildStats {
                estimated_prompt_tokens: 3,
                messages_count: 1,
                ..PromptBuildStats::default()
            },
        };
        let mut events = Vec::<ModelDeltaEvent>::new();

        let outcome = orchestrator
            .run_turn(&snapshot, initial_prompt_bundle, |event| events.push(event))
            .await;

        assert!(!outcome.failed);
        assert_eq!(outcome.action_call_count, 1);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    ModelDeltaEvent::StreamNote(note) if note.phase == "agent.turn.attempt"
                ))
                .count(),
            2
        );
        assert_eq!(
            fake_adapter
                .prompt_message_counts
                .lock()
                .expect("prompt counts mutex")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn run_turn_short_circuits_when_model_adapter_is_unavailable() {
        let orchestrator = AgentOrchestrator::with_model_adapter(Arc::new(
            FakeModelAdapter::unavailable("missing API key"),
        ));
        let snapshot = test_snapshot();

        let outcome = orchestrator
            .run_turn(&snapshot, PromptMessageBundle::default(), |_| {})
            .await;

        assert!(outcome.failed);
        assert_eq!(outcome.failure_code, "agent_init_error");
        assert!(outcome.failure_message.contains("model adapter `fake`"));
    }
}
