use crate::agent::types::{PromptEvent, PromptInput};

use super::timeline::TimelineEvent;
use super::util::{preview_to_inline, serialize_pretty_json, truncate_inline};
use super::{MAX_INLINE_TEXT_CHARS, MAX_LOOKUP_PAYLOAD_CHARS};

pub(super) fn build_harness_contract_block(input: &PromptInput) -> String {
    [
        "## Harness Contract".to_string(),
        format!(
            "runtime_version: {}",
            input.stable_prefix.harness_contract.runtime_version
        ),
        format!(
            "contract_schema_version: {}",
            input.stable_prefix.harness_contract.contract_schema_version
        ),
        String::new(),
        "You may emit assistant text and/or action executions in the same turn.".to_string(),
        "Use only actions listed in the Session Baseline capability surface.".to_string(),
        "Use canonical action ids in the format env__action.".to_string(),
        "Execution requests default to `await` semantics.".to_string(),
        "Request `detach` only when an action's `mode_support` is `await_or_detach`.".to_string(),
        "If an action is `await_only`, requesting `detach` will be rejected.".to_string(),
        "Use Resolved Payload Lookups when present before issuing additional payload fetches.".to_string(),
        "Do not assume current time unless an execution result or event provides it explicitly.".to_string(),
        "Do not assume live environment state unless an execution result or event provides it explicitly.".to_string(),
        "Action input schemas are enforced by the runtime; provide exact argument shapes.".to_string(),
        "For optional action arguments, omit fields you do not need; never send empty placeholder strings.".to_string(),
        "Avoid unbounded execution chaining. When evidence is sufficient, provide a direct assistant report to the user.".to_string(),
    ]
    .join("\n")
}

pub(super) fn build_identity_envelope_block(input: &PromptInput) -> String {
    let lines = [
        "## Identity Envelope".to_string(),
        format!(
            "schema_version: {}",
            input.stable_prefix.identity_envelope.schema_version
        ),
        format!(
            "source_revision: {}",
            input.stable_prefix.identity_envelope.source_revision
        ),
        "material_json:".to_string(),
        serialize_pretty_json(&input.stable_prefix.identity_envelope.material),
    ];
    lines.join("\n")
}

pub(super) fn build_session_baseline_block(input: &PromptInput) -> String {
    let mut lines = vec![
        "## Session Baseline".to_string(),
        "### Session Anchor".to_string(),
        format!(
            "session_id: {}",
            input
                .stable_prefix
                .session_baseline
                .session_anchor
                .session_id
        ),
        format!(
            "started_at_unix_ms: {}",
            input
                .stable_prefix
                .session_baseline
                .session_anchor
                .started_at_unix_ms
        ),
        String::new(),
        "### Capability Surface".to_string(),
    ];
    let mut environments = input
        .stable_prefix
        .session_baseline
        .capability_surface
        .environments
        .clone();
    environments.sort_by(|a, b| a.id.cmp(&b.id));
    if environments.is_empty() {
        lines.push("(none)".to_string());
    } else {
        for environment in environments {
            lines.push(format!(
                "- id={} name={} description={}",
                environment.id, environment.name, environment.description
            ));
            let mut actions = environment.actions;
            actions.sort_by(|a, b| a.action_id.cmp(&b.action_id));
            if actions.is_empty() {
                lines.push("  actions: (none)".to_string());
            } else {
                lines.push("  actions:".to_string());
                for action in actions {
                    if action.discovery {
                        lines.push(format!(
                            "  - {} [{}] (discovery): {}",
                            action.action_id,
                            action.mode_support.as_str(),
                            action.description
                        ));
                    } else {
                        lines.push(format!(
                            "  - {} [{}]: {}",
                            action.action_id,
                            action.mode_support.as_str(),
                            action.description
                        ));
                    }
                }
            }

            let mut recipes = environment.recipes;
            recipes.sort_by(|a, b| a.title.cmp(&b.title));
            if recipes.is_empty() {
                lines.push("  recipes: (none)".to_string());
            } else {
                lines.push("  recipes:".to_string());
                for recipe in recipes {
                    lines.push(format!("  - {}:", recipe.title));
                    for step in recipe.steps {
                        lines.push(format!("    - {}", step));
                    }
                }
            }
        }
    }

    lines.push(String::new());
    lines.push("### Participant Envelope".to_string());
    lines.push(format!(
        "schema_version: {}",
        input
            .stable_prefix
            .session_baseline
            .participant_envelope
            .schema_version
    ));
    lines.push(format!(
        "source_revision: {}",
        input
            .stable_prefix
            .session_baseline
            .participant_envelope
            .source_revision
    ));
    lines.push("material_json:".to_string());
    lines.push(serialize_pretty_json(
        &input
            .stable_prefix
            .session_baseline
            .participant_envelope
            .material,
    ));

    lines.join("\n")
}

pub(super) fn build_tail_event_lines(input: &PromptInput) -> Vec<String> {
    let mut lines = Vec::new();

    for event in &input.pending_events {
        lines.extend(render_pending_prompt_event_lines(event));
    }

    if lines.is_empty() {
        lines.push("event_log_empty".to_string());
    }

    lines
}

pub(super) fn render_event_transcript_lines(
    summaries: &[String],
    events: &[TimelineEvent],
    tail_event_lines: &[String],
) -> Vec<String> {
    let mut lines = Vec::new();
    if !summaries.is_empty() {
        lines.push("### Compaction Summaries".to_string());
        for summary in summaries {
            lines.push(format!("- {}", summary));
        }
        lines.push(String::new());
    }

    if events.is_empty() {
        lines.push("history_events=(none)".to_string());
    } else {
        for event in events {
            lines.push(event.line.clone());
        }
    }

    if !tail_event_lines.is_empty() {
        lines.push(String::new());
        lines.push("### Pending Events".to_string());
        for line in tail_event_lines {
            lines.push(line.clone());
        }
    }
    lines
}

fn render_pending_prompt_event_lines(event: &PromptEvent) -> Vec<String> {
    match event {
        PromptEvent::UserMessage(payload) => vec![format!(
            "pending_trigger user_message user={} text={}",
            payload.user_id,
            truncate_inline(&payload.text, MAX_INLINE_TEXT_CHARS)
        )],
        PromptEvent::AwaitedExecutionSucceeded(payload) => vec![format!(
            "pending_trigger awaited_execution_succeeded execution_id={} action_id={} payload_preview={}",
            payload.execution_id,
            payload.action_id,
            preview_to_inline(&payload.payload_preview)
        )],
        PromptEvent::AwaitedExecutionFailed(payload) => vec![format!(
            "pending_trigger awaited_execution_failed execution_id={} action_id={} message={}{}",
            payload.execution_id,
            payload.action_id,
            truncate_inline(&payload.message, MAX_INLINE_TEXT_CHARS),
            payload
                .payload_preview
                .as_ref()
                .map(|preview| format!(" payload_preview={}", preview_to_inline(preview)))
                .unwrap_or_default()
        )],
        PromptEvent::ExecutionDetached(payload) => vec![format!(
            "pending_trigger execution_detached execution_id={} action_id={}",
            payload.execution_id, payload.action_id
        )],
        PromptEvent::DetachedExecutionSucceeded(payload) => vec![format!(
            "pending_trigger detached_execution_succeeded execution_id={} action_id={} payload_preview={}",
            payload.execution_id,
            payload.action_id,
            preview_to_inline(&payload.payload_preview)
        )],
        PromptEvent::DetachedExecutionFailed(payload) => vec![format!(
            "pending_trigger detached_execution_failed execution_id={} action_id={} message={}{}",
            payload.execution_id,
            payload.action_id,
            truncate_inline(&payload.message, MAX_INLINE_TEXT_CHARS),
            payload
                .payload_preview
                .as_ref()
                .map(|preview| format!(" payload_preview={}", preview_to_inline(preview)))
                .unwrap_or_default()
        )],
        PromptEvent::ExecutionRejected(payload) => vec![format!(
            "pending_trigger execution_rejected execution_id={} action_id={} message={}",
            payload.execution_id,
            payload.action_id,
            truncate_inline(&payload.message, MAX_INLINE_TEXT_CHARS)
        )],
        PromptEvent::PayloadLookupAvailable(payload) => vec![
            format!(
                "resolved_payload_lookup lookup_execution_id={} execution_id={} part={} offset={} next_offset={} full_bytes={} source_truncated={} injected_truncated={} injected_omitted_bytes={}",
                payload.lookup_execution_id,
                payload.execution_id,
                payload.part,
                payload.offset,
                payload
                    .next_offset
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-1".to_string()),
                payload.full_bytes,
                payload.source_truncated,
                payload.injected_truncated,
                payload.injected_omitted_bytes
            ),
            format!(
                "payload_chunk {}",
                truncate_inline(&payload.payload_chunk, MAX_LOOKUP_PAYLOAD_CHARS)
            ),
        ],
        PromptEvent::RetryFeedback(payload) => vec![format!(
            "retry_feedback {}",
            truncate_inline(&payload.content, MAX_LOOKUP_PAYLOAD_CHARS)
        )],
        PromptEvent::Heartbeat => vec!["pending_trigger heartbeat".to_string()],
        PromptEvent::Cron(payload) => {
            vec![format!("pending_trigger cron key={}", payload.key)]
        }
        PromptEvent::RefreshProfile(payload) => vec![format!(
            "pending_trigger refresh_profile scope={} user_id={}",
            payload.scope, payload.user_id
        )],
        PromptEvent::AssistantOutput(_) | PromptEvent::ExecutionRequested(_) => Vec::new(),
    }
}
