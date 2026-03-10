use crate::agent::types::{
    PromptAssistantOutput, PromptEvent, PromptExecutionDetached, PromptExecutionRequested,
    PromptExecutionSucceeded, PromptInput, PromptPayloadLookupAvailable, PromptStablePrefix,
    PromptUserMessage,
};
use crate::agent::{
    ActionModeSupportContract, CapabilityAction, CapabilityDomain, CapabilityRecipe,
    CapabilitySurface, CompiledPrompt, HarnessContract, IdentityEnvelope, ParticipantEnvelope,
    SessionAnchor, SessionBaseline, SummaryBlockRef,
};
use crate::history::PayloadPreview;
use crate::util::default_agent_profile;
use serde_json::json;

use super::PromptCompiler;

fn sample_preview(lookup_ref: &str) -> PayloadPreview {
    PayloadPreview {
        head: "[]".to_string(),
        tail: String::new(),
        full_bytes: 12,
        head_bytes: 2,
        tail_bytes: 0,
        truncated: false,
        omitted_bytes: 0,
        lookup_ref: lookup_ref.to_string(),
    }
}

fn base_input() -> PromptInput {
    let agent_profile = default_agent_profile("agent-default");
    PromptInput {
        stable_prefix: PromptStablePrefix {
            harness_contract: HarnessContract {
                runtime_version: "0.1.0".to_string(),
                contract_schema_version: 1,
            },
            identity_envelope: IdentityEnvelope {
                schema_version: 1,
                source_revision: format!(
                    "{}@spec:{}@updated:{}",
                    &agent_profile.agent_id,
                    agent_profile.spec_version,
                    agent_profile.updated_at_unix_ms
                ),
                material: serde_json::from_str(&agent_profile.material_json)
                    .expect("agent material json"),
            },
            session_baseline: SessionBaseline {
                session_anchor: SessionAnchor {
                    session_id: "session-1".to_string(),
                    started_at_unix_ms: 1_765_000_000_000,
                },
                capability_surface: CapabilitySurface {
                    capability_domains: vec![
                        CapabilityDomain {
                            id: "filesystem".to_string(),
                            name: "Filesystem".to_string(),
                            description: "Stateful filesystem environment rooted at a base path."
                                .to_string(),
                            actions: vec![CapabilityAction {
                                action_id: "filesystem__list".to_string(),
                                description:
                                    "List directory entries for a non-empty relative path."
                                        .to_string(),
                                mode_support: ActionModeSupportContract::AwaitOnly,
                                discovery: false,
                            }],
                            recipes: vec![CapabilityRecipe {
                                title: "Find files".to_string(),
                                steps: vec![
                                    "Call filesystem__list with path '.'.".to_string(),
                                    "Call filesystem__read for selected files.".to_string(),
                                ],
                            }],
                        },
                        CapabilityDomain {
                            id: "system".to_string(),
                            name: "System".to_string(),
                            description: "Inspect runtime context and metadata.".to_string(),
                            actions: vec![CapabilityAction {
                                action_id: "system__get_time".to_string(),
                                description: "Get current server time context.".to_string(),
                                mode_support: ActionModeSupportContract::AwaitOnly,
                                discovery: true,
                            }],
                            recipes: vec![],
                        },
                    ],
                },
                participant_envelope: ParticipantEnvelope {
                    schema_version: 1,
                    source_revision: "user-default@1765000000000".to_string(),
                    material: json!({
                        "participants": [{
                            "user_id": "user-default",
                            "name": "User Default",
                            "nickname": "user-default",
                            "preferences": {},
                            "memory": {
                                "long_term": ""
                            }
                        }]
                    }),
                },
            },
        },
        transcript_events: vec![],
        pending_events: vec![],
        compaction_blocks: vec![],
    }
}

fn compile_input(input: &PromptInput) -> CompiledPrompt {
    PromptCompiler::new().compile(input)
}

#[test]
fn bundle_contains_layered_messages_and_stats() {
    let input = base_input();

    let bundle = compile_input(&input);
    assert!(bundle.messages.len() >= 4);
    assert_eq!(bundle.diagnostics.messages_count, bundle.messages.len());
    assert!(bundle.diagnostics.estimated_prompt_tokens > 0);
    assert!(!bundle.diagnostics.stable_prefix_hash.is_empty());

    let debug_prompt = bundle.as_debug_prompt();
    assert!(debug_prompt.contains("# Harness Contract"));
    assert!(debug_prompt.contains("# Identity Envelope"));
    assert!(debug_prompt.contains("# Session Baseline"));
    assert!(debug_prompt.contains("### Filesystem (`filesystem`)"));
    assert!(debug_prompt.contains("- `filesystem__list` · `await_only`"));
    assert!(debug_prompt.contains("- `system__get_time` · `await_only` · `discovery`"));
    assert!(debug_prompt.contains("## Your Task"));
    assert!(debug_prompt.contains("## Response vs Execution"));
    assert!(debug_prompt.contains("## Execution Rules"));
    assert!(debug_prompt.contains("## Evidence and Payloads"));
    assert!(debug_prompt.contains("## Failure Handling"));
    assert!(debug_prompt.contains("## Response Style"));
    assert!(debug_prompt.contains(
        "- For optional arguments, omit fields you do not need and never send empty placeholder strings."
    ));
    assert!(debug_prompt.contains(
        "- `execution_rejected` means the runtime did not accept the requested execution; revise the request instead of assuming it ran."
    ));
    assert!(debug_prompt.contains(
        "- Do not continue chaining actions for too long without responding to the user."
    ));
    assert!(debug_prompt.contains("## Identity Material"));
    assert!(debug_prompt.contains("```md"));
    assert!(debug_prompt.contains("## Identity"));
    assert!(debug_prompt.contains("## Participant Envelope"));
    assert!(debug_prompt.contains("### Participant Material"));
    assert!(debug_prompt.contains("## user-default"));
    assert!(debug_prompt.contains("#### Recipes"));
    assert!(debug_prompt.contains("##### Find files"));
    assert!(!debug_prompt.contains("## Time Context"));
    assert!(debug_prompt.contains("## Event Transcript"));
    assert!(!debug_prompt.contains("## Turn Input"));
    assert!(!debug_prompt.contains("## Resolved Payload Lookups"));
}

#[test]
fn stable_prefix_hash_is_unchanged_by_tail_event_changes() {
    let input = base_input();
    let bundle = compile_input(&input);

    let mut next_input = input.clone();
    next_input.pending_events = vec![
        PromptEvent::UserMessage(PromptUserMessage {
            user_id: "user-default".to_string(),
            text: "what changed?".to_string(),
        }),
        PromptEvent::PayloadLookupAvailable(PromptPayloadLookupAvailable {
            lookup_execution_id: "lookup-1".to_string(),
            execution_id: "execution-1".to_string(),
            part: "result".to_string(),
            offset: 0,
            next_offset: Some(120),
            full_bytes: 1024,
            source_truncated: true,
            payload_chunk: "{\"ok\":true}".to_string(),
            injected_truncated: false,
            injected_omitted_bytes: 0,
        }),
        PromptEvent::RetryFeedback(PromptAssistantOutput {
            content: "retry with better args".to_string(),
        }),
    ];
    let next_bundle = compile_input(&next_input);

    assert_eq!(
        bundle.diagnostics.stable_prefix_hash,
        next_bundle.diagnostics.stable_prefix_hash
    );
}

#[test]
fn event_transcript_includes_pending_events_and_lookup_availability() {
    let mut input = base_input();
    input.pending_events = vec![
        PromptEvent::UserMessage(PromptUserMessage {
            user_id: "user-default".to_string(),
            text: "inspect the payload".to_string(),
        }),
        PromptEvent::PayloadLookupAvailable(PromptPayloadLookupAvailable {
            lookup_execution_id: "lookup-1".to_string(),
            execution_id: "execution-1".to_string(),
            part: "result".to_string(),
            offset: 0,
            next_offset: Some(120),
            full_bytes: 1024,
            source_truncated: true,
            payload_chunk: "{\"ok\":true}".to_string(),
            injected_truncated: false,
            injected_omitted_bytes: 0,
        }),
    ];

    let bundle = compile_input(&input);
    let debug_prompt = bundle.as_debug_prompt();

    assert!(debug_prompt.contains("## Pending Inputs"));
    assert!(debug_prompt.contains(
        "resolved_payload_lookup lookup_execution_id=lookup-1 execution_id=execution-1 part=result offset=0"
    ));
    assert!(debug_prompt.contains("payload_chunk {\"ok\":true}"));
}

#[test]
fn transcript_events_drive_execution_requests_and_outcomes() {
    let mut input = base_input();
    input.transcript_events = vec![
        PromptEvent::UserMessage(PromptUserMessage {
            user_id: "user-default".to_string(),
            text: "show me the repo files".to_string(),
        }),
        PromptEvent::ExecutionRequested(PromptExecutionRequested {
            execution_id: "execution-1".to_string(),
            action_id: "filesystem__list".to_string(),
            execution_mode: "await".to_string(),
            args_preview: sample_preview("execution://execution-1/args"),
        }),
        PromptEvent::AwaitedExecutionSucceeded(PromptExecutionSucceeded {
            execution_id: "execution-1".to_string(),
            action_id: "filesystem__list".to_string(),
            payload_preview: sample_preview("execution://execution-1/result"),
        }),
    ];

    let bundle = compile_input(&input);
    let debug_prompt = bundle.as_debug_prompt();

    assert!(debug_prompt.contains("user_message user=user-default text=show me the repo files"));
    assert!(debug_prompt.contains(
        "execution_requested execution_id=execution-1 action_id=filesystem__list mode=await"
    ));
    assert!(debug_prompt.contains(
        "awaited_execution_succeeded execution_id=execution-1 action_id=filesystem__list"
    ));
}

#[test]
fn transcript_preserves_execution_event_order() {
    let mut input = base_input();
    input.transcript_events = vec![
        PromptEvent::ExecutionRequested(PromptExecutionRequested {
            execution_id: "execution-7".to_string(),
            action_id: "shell__run".to_string(),
            execution_mode: "detach".to_string(),
            args_preview: sample_preview("execution://execution-7/args"),
        }),
        PromptEvent::ExecutionDetached(PromptExecutionDetached {
            execution_id: "execution-7".to_string(),
            action_id: "shell__run".to_string(),
        }),
        PromptEvent::DetachedExecutionSucceeded(PromptExecutionSucceeded {
            execution_id: "execution-7".to_string(),
            action_id: "shell__run".to_string(),
            payload_preview: sample_preview("execution://execution-7/result"),
        }),
    ];

    let debug_prompt = compile_input(&input).as_debug_prompt();
    let execution_request_index = debug_prompt
        .find("execution_requested execution_id=execution-7 action_id=shell__run mode=detach")
        .expect("execution_requested line");
    let detached_index = debug_prompt
        .find("execution_detached execution_id=execution-7 action_id=shell__run")
        .expect("execution_detached line");
    let success_index = debug_prompt
        .find("detached_execution_succeeded execution_id=execution-7 action_id=shell__run")
        .expect("detached success line");

    assert!(execution_request_index < detached_index);
    assert!(detached_index < success_index);
}

#[test]
fn transcript_messages_remain_prefix_append_only_as_history_grows() {
    let mut base_transcript_input = base_input();
    base_transcript_input.transcript_events = vec![
        PromptEvent::AssistantOutput(PromptAssistantOutput {
            content: "earlier answer".to_string(),
        }),
        PromptEvent::UserMessage(PromptUserMessage {
            user_id: "user-default".to_string(),
            text: "next question".to_string(),
        }),
    ];

    let base_bundle = compile_input(&base_transcript_input);
    let base_prompt = base_bundle.as_debug_prompt();
    assert!(!base_prompt.contains("## Pending Inputs"));

    let base_event_message = base_bundle
        .messages
        .iter()
        .find(|message| message.label == "event_transcript")
        .expect("event transcript message")
        .content
        .clone();
    assert!(base_event_message.contains("assistant_output content=earlier answer"));
    assert!(base_event_message.contains("user_message user=user-default text=next question"));

    let mut grown_transcript_input = base_input();
    grown_transcript_input.transcript_events = vec![
        PromptEvent::AssistantOutput(PromptAssistantOutput {
            content: "earlier answer".to_string(),
        }),
        PromptEvent::UserMessage(PromptUserMessage {
            user_id: "user-default".to_string(),
            text: "next question".to_string(),
        }),
        PromptEvent::AssistantOutput(PromptAssistantOutput {
            content: "latest answer".to_string(),
        }),
    ];

    let grown_event_message = compile_input(&grown_transcript_input)
        .messages
        .into_iter()
        .find(|message| message.label == "event_transcript")
        .expect("event transcript message")
        .content;
    assert!(grown_event_message.starts_with(&base_event_message));
}

#[test]
fn bundle_includes_session_compaction_summaries() {
    let mut input = base_input();
    input.compaction_blocks = vec![SummaryBlockRef {
        id: "history-summary-000024".to_string(),
        source_range_start: 0,
        source_range_end: 24,
        summary_text: "history-summary-000024 source=[0,24) events=24 user_message=3 assistant_output=2 execution_requested=4 awaited_execution_succeeded=4 awaited_execution_failed=0 execution_detached=0 detached_execution_succeeded=0 detached_execution_failed=0 execution_rejected=0 refresh_profile=1 heartbeat=0 cron=0 statuses=[succeeded:4] actions=[filesystem__list] users=[user-default]".to_string(),
        created_at_unix_ms: 1_765_000_000_000,
    }];

    let bundle = compile_input(&input);
    let debug_prompt = bundle.as_debug_prompt();

    assert!(debug_prompt.contains("history-summary-000024 source=[0,24)"));
    assert!(bundle.diagnostics.compaction_applied);
    assert!(
        bundle
            .diagnostics
            .compaction_reason
            .contains("session_summary_blocks=1")
    );
}
