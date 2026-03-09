use crate::agent::types::{
    PromptAssistantOutput, PromptCron, PromptEvent, PromptExecutionDetached, PromptExecutionFailed,
    PromptExecutionRejected, PromptExecutionRequested, PromptExecutionSucceeded, PromptInput,
    PromptPayloadLookupAvailable, PromptRefreshProfile, PromptStablePrefix, PromptUserMessage,
    TurnSnapshot,
};
use crate::history::build_payload_preview;
use crate::history::{HistoryEvent, HistoryEventKind};
use fathom_protocol::pb;

pub(crate) fn build_prompt_input(
    snapshot: &TurnSnapshot,
    retry_feedback: Option<&str>,
) -> PromptInput {
    let transcript_events = snapshot
        .recent_history
        .iter()
        .filter_map(prompt_event_from_history_event)
        .collect::<Vec<_>>();
    let mut pending_events = snapshot
        .triggers
        .iter()
        .filter_map(prompt_event_from_trigger)
        .collect::<Vec<_>>();
    pending_events.extend(snapshot.resolved_payload_lookups.iter().map(|lookup| {
        PromptEvent::PayloadLookupAvailable(PromptPayloadLookupAvailable {
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
    }));
    if let Some(feedback) = retry_feedback {
        pending_events.push(PromptEvent::RetryFeedback(PromptAssistantOutput {
            content: feedback.to_string(),
        }));
    }

    PromptInput {
        stable_prefix: PromptStablePrefix {
            harness_contract: snapshot.harness_contract.clone(),
            identity_envelope: snapshot.identity_envelope.clone(),
            session_baseline: snapshot.session_baseline.clone(),
        },
        transcript_events,
        pending_events,
        compaction_blocks: snapshot.compaction.summary_blocks.clone(),
    }
}

fn prompt_event_from_history_event(event: &HistoryEvent) -> Option<PromptEvent> {
    match &event.kind {
        HistoryEventKind::TriggerUserMessage(payload) => {
            Some(PromptEvent::UserMessage(PromptUserMessage {
                user_id: event.actor_id.clone(),
                text: payload.text.clone(),
            }))
        }
        HistoryEventKind::AssistantOutput(payload) => {
            Some(PromptEvent::AssistantOutput(PromptAssistantOutput {
                content: payload.content.clone(),
            }))
        }
        HistoryEventKind::ExecutionRequested(payload) => {
            Some(PromptEvent::ExecutionRequested(PromptExecutionRequested {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
                execution_mode: payload.execution_mode.clone(),
                args_preview: payload.args_preview.clone(),
            }))
        }
        HistoryEventKind::AwaitedExecutionSucceeded(payload) => Some(
            PromptEvent::AwaitedExecutionSucceeded(PromptExecutionSucceeded {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
                payload_preview: payload.payload_preview.clone(),
            }),
        ),
        HistoryEventKind::AwaitedExecutionFailed(payload) => {
            Some(PromptEvent::AwaitedExecutionFailed(PromptExecutionFailed {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
                message: payload.message.clone(),
                payload_preview: payload.payload_preview.clone(),
            }))
        }
        HistoryEventKind::ExecutionDetached(payload) => {
            Some(PromptEvent::ExecutionDetached(PromptExecutionDetached {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
            }))
        }
        HistoryEventKind::DetachedExecutionSucceeded(payload) => Some(
            PromptEvent::DetachedExecutionSucceeded(PromptExecutionSucceeded {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
                payload_preview: payload.payload_preview.clone(),
            }),
        ),
        HistoryEventKind::DetachedExecutionFailed(payload) => Some(
            PromptEvent::DetachedExecutionFailed(PromptExecutionFailed {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
                message: payload.message.clone(),
                payload_preview: payload.payload_preview.clone(),
            }),
        ),
        HistoryEventKind::ExecutionRejected(payload) => {
            Some(PromptEvent::ExecutionRejected(PromptExecutionRejected {
                execution_id: event.actor_id.clone(),
                action_id: payload.canonical_action_id.clone(),
                message: payload.message.clone(),
            }))
        }
        HistoryEventKind::TriggerUnknown
        | HistoryEventKind::TriggerHeartbeat
        | HistoryEventKind::TriggerCron(_)
        | HistoryEventKind::TriggerRefreshProfile(_) => None,
    }
}

fn prompt_event_from_trigger(trigger: &pb::Trigger) -> Option<PromptEvent> {
    let kind = trigger.kind.as_ref()?;
    match kind {
        pb::trigger::Kind::UserMessage(message) => {
            Some(PromptEvent::UserMessage(PromptUserMessage {
                user_id: message.user_id.clone(),
                text: message.text.clone(),
            }))
        }
        pb::trigger::Kind::ExecutionUpdate(update) => prompt_event_from_execution_update(update),
        pb::trigger::Kind::Heartbeat(_) => Some(PromptEvent::Heartbeat),
        pb::trigger::Kind::Cron(cron) => Some(PromptEvent::Cron(PromptCron {
            key: cron.key.clone(),
        })),
        pb::trigger::Kind::RefreshProfile(refresh) => {
            Some(PromptEvent::RefreshProfile(PromptRefreshProfile {
                scope: refresh.scope.to_string(),
                user_id: refresh.user_id.clone(),
            }))
        }
    }
}

fn prompt_event_from_execution_update(update: &pb::ExecutionUpdateTrigger) -> Option<PromptEvent> {
    let payload_preview = if update.payload_message.trim().is_empty() {
        None
    } else {
        Some(build_payload_preview(
            &update.payload_message,
            format!("execution://{}/result", update.execution_id),
        ))
    };
    let kind = pb::ExecutionUpdateKind::try_from(update.kind)
        .unwrap_or(pb::ExecutionUpdateKind::Unspecified);
    match kind {
        pb::ExecutionUpdateKind::AwaitedExecutionSucceeded => payload_preview.map(|preview| {
            PromptEvent::AwaitedExecutionSucceeded(PromptExecutionSucceeded {
                execution_id: update.execution_id.clone(),
                action_id: update.action_id.clone(),
                payload_preview: preview,
            })
        }),
        pb::ExecutionUpdateKind::AwaitedExecutionFailed => {
            Some(PromptEvent::AwaitedExecutionFailed(PromptExecutionFailed {
                execution_id: update.execution_id.clone(),
                action_id: update.action_id.clone(),
                message: update.message.clone(),
                payload_preview,
            }))
        }
        pb::ExecutionUpdateKind::ExecutionDetached => {
            Some(PromptEvent::ExecutionDetached(PromptExecutionDetached {
                execution_id: update.execution_id.clone(),
                action_id: update.action_id.clone(),
            }))
        }
        pb::ExecutionUpdateKind::DetachedExecutionSucceeded => payload_preview.map(|preview| {
            PromptEvent::DetachedExecutionSucceeded(PromptExecutionSucceeded {
                execution_id: update.execution_id.clone(),
                action_id: update.action_id.clone(),
                payload_preview: preview,
            })
        }),
        pb::ExecutionUpdateKind::DetachedExecutionFailed => Some(
            PromptEvent::DetachedExecutionFailed(PromptExecutionFailed {
                execution_id: update.execution_id.clone(),
                action_id: update.action_id.clone(),
                message: update.message.clone(),
                payload_preview,
            }),
        ),
        pb::ExecutionUpdateKind::ExecutionRejected => {
            Some(PromptEvent::ExecutionRejected(PromptExecutionRejected {
                execution_id: update.execution_id.clone(),
                action_id: update.action_id.clone(),
                message: update.message.clone(),
            }))
        }
        pb::ExecutionUpdateKind::Unspecified => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::prompt_input_builder::build_prompt_input;
    use crate::agent::types::{
        ActionModeSupportSnapshot, CapabilityActionSnapshot, CapabilityEnvironmentSnapshot,
        CapabilityRecipeSnapshot, CapabilitySurfaceSnapshot, HarnessContractSnapshot,
        IdentityEnvelopeSnapshot, ParticipantEnvelopeSnapshot, PromptEvent,
        ResolvedPayloadLookupHint, SessionAnchorSnapshot, SessionBaselineSnapshot,
        SessionCompactionSnapshot, TurnSnapshot,
    };
    use crate::history::HistoryEvent;
    use crate::history::schema::{HistoryActorKind, HistoryEventKind, UserMessageHistoryPayload};
    use crate::util::default_agent_profile;
    use fathom_protocol::pb;
    use serde_json::json;

    fn base_snapshot(recent_history: Vec<HistoryEvent>) -> TurnSnapshot {
        let agent_profile = default_agent_profile("agent-default");
        TurnSnapshot {
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
                        actions: vec![CapabilityActionSnapshot {
                            action_id: "filesystem__list".to_string(),
                            description: "List directory entries for a non-empty relative path."
                                .to_string(),
                            mode_support: ActionModeSupportSnapshot::AwaitOnly,
                            discovery: false,
                        }],
                        recipes: vec![CapabilityRecipeSnapshot {
                            title: "Find files".to_string(),
                            steps: vec![
                                "Call filesystem__list with path '.'.".to_string(),
                                "Call filesystem__read for selected files.".to_string(),
                            ],
                        }],
                    }],
                },
                participant_envelope: ParticipantEnvelopeSnapshot {
                    schema_version: 1,
                    source_revision: "user-default@1765000000000".to_string(),
                    material: json!({
                        "participants": [{
                            "user_id": "user-default",
                            "name": "User Default",
                            "nickname": "user-default",
                            "preferences_json": "{}",
                            "user_md": "USER.md",
                            "long_term_memory_md": ""
                        }]
                    }),
                },
            },
            resolved_payload_lookups: vec![],
            triggers: vec![],
            recent_history,
            compaction: SessionCompactionSnapshot::default(),
        }
    }

    #[test]
    fn build_prompt_input_normalizes_transcript_and_pending_events() {
        let mut snapshot = base_snapshot(vec![HistoryEvent {
            ts_unix_ms: 10,
            actor_kind: HistoryActorKind::User,
            actor_id: "user-default".to_string(),
            profile_ref: "user:user-default@t0".to_string(),
            kind: HistoryEventKind::TriggerUserMessage(UserMessageHistoryPayload {
                text: "hello".to_string(),
            }),
        }]);
        snapshot.triggers = vec![pb::Trigger {
            trigger_id: "trigger-1".to_string(),
            created_at_unix_ms: 1_765_000_000_100,
            kind: Some(pb::trigger::Kind::Heartbeat(pb::HeartbeatTrigger {})),
        }];
        snapshot.resolved_payload_lookups = vec![ResolvedPayloadLookupHint {
            lookup_execution_id: "lookup-1".to_string(),
            execution_id: "execution-1".to_string(),
            part: "result".to_string(),
            offset: 0,
            next_offset: Some(12),
            full_bytes: 42,
            source_truncated: false,
            payload_chunk: "{\"ok\":true}".to_string(),
            injected_truncated: false,
            injected_omitted_bytes: 0,
        }];

        let input = build_prompt_input(&snapshot, Some("retry now"));

        assert_eq!(input.transcript_events.len(), 1);
        assert!(matches!(
            input.transcript_events.first(),
            Some(PromptEvent::UserMessage(_))
        ));
        assert_eq!(input.pending_events.len(), 3);
        assert!(
            input
                .pending_events
                .iter()
                .any(|event| matches!(event, PromptEvent::Heartbeat))
        );
        assert!(
            input
                .pending_events
                .iter()
                .any(|event| matches!(event, PromptEvent::PayloadLookupAvailable(_)))
        );
        assert!(
            input
                .pending_events
                .iter()
                .any(|event| matches!(event, PromptEvent::RetryFeedback(_)))
        );
    }

    #[test]
    fn build_prompt_input_preserves_execution_update_trigger_order_and_normalizes_variants() {
        let mut snapshot = base_snapshot(vec![]);
        snapshot.triggers = vec![
            pb::Trigger {
                trigger_id: "trigger-1".to_string(),
                created_at_unix_ms: 1_765_000_000_100,
                kind: Some(pb::trigger::Kind::ExecutionUpdate(
                    pb::ExecutionUpdateTrigger {
                        execution_id: "execution-1".to_string(),
                        action_id: "filesystem__list".to_string(),
                        kind: pb::ExecutionUpdateKind::AwaitedExecutionSucceeded as i32,
                        message: String::new(),
                        payload_message: "{\"entries\":[\"src\"]}".to_string(),
                    },
                )),
            },
            pb::Trigger {
                trigger_id: "trigger-2".to_string(),
                created_at_unix_ms: 1_765_000_000_200,
                kind: Some(pb::trigger::Kind::ExecutionUpdate(
                    pb::ExecutionUpdateTrigger {
                        execution_id: "execution-2".to_string(),
                        action_id: "shell__run".to_string(),
                        kind: pb::ExecutionUpdateKind::DetachedExecutionFailed as i32,
                        message: "process exited with status 1".to_string(),
                        payload_message: "stderr: boom".to_string(),
                    },
                )),
            },
            pb::Trigger {
                trigger_id: "trigger-3".to_string(),
                created_at_unix_ms: 1_765_000_000_300,
                kind: Some(pb::trigger::Kind::ExecutionUpdate(
                    pb::ExecutionUpdateTrigger {
                        execution_id: "execution-3".to_string(),
                        action_id: "shell__run".to_string(),
                        kind: pb::ExecutionUpdateKind::ExecutionRejected as i32,
                        message: "detach is not allowed for shell__run".to_string(),
                        payload_message: String::new(),
                    },
                )),
            },
        ];

        let input = build_prompt_input(&snapshot, None);

        assert_eq!(input.pending_events.len(), 3);
        assert!(matches!(
            input.pending_events.first(),
            Some(PromptEvent::AwaitedExecutionSucceeded(item))
                if item.execution_id == "execution-1"
                    && item.action_id == "filesystem__list"
                    && item.payload_preview.lookup_ref == "execution://execution-1/result"
                    && item.payload_preview.head.contains("\"src\"")
        ));
        assert!(matches!(
            input.pending_events.get(1),
            Some(PromptEvent::DetachedExecutionFailed(item))
                if item.execution_id == "execution-2"
                    && item.action_id == "shell__run"
                    && item.message == "process exited with status 1"
                    && item.payload_preview.as_ref().is_some_and(|preview|
                        preview.lookup_ref == "execution://execution-2/result"
                            && preview.head.contains("stderr: boom"))
        ));
        assert!(matches!(
            input.pending_events.get(2),
            Some(PromptEvent::ExecutionRejected(item))
                if item.execution_id == "execution-3"
                    && item.action_id == "shell__run"
                    && item.message == "detach is not allowed for shell__run"
        ));
    }
}
