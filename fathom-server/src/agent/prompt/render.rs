use crate::agent::types::{PromptEvent, PromptInput};
use serde_json::{Map, Value};

use super::timeline::TimelineEvent;
use super::util::{preview_to_inline, truncate_inline};
use super::{MAX_INLINE_TEXT_CHARS, MAX_LOOKUP_PAYLOAD_CHARS};

pub(super) fn build_harness_contract_block(input: &PromptInput) -> String {
    [
        "# Harness Contract".to_string(),
        format!(
            "- `runtime_version`: {}",
            input.stable_prefix.harness_contract.runtime_version
        ),
        format!(
            "- `contract_schema_version`: {}",
            input.stable_prefix.harness_contract.contract_schema_version
        ),
        String::new(),
        "## Your Task".to_string(),
        "You operate inside a session runtime that provides a stable session prefix, an additive event transcript, and a capability surface of callable actions.".to_string(),
        "Your job is to choose the next best move for the session.".to_string(),
        String::new(),
        "## Allowed Outputs".to_string(),
        "- You may emit assistant text and/or action executions in the same turn.".to_string(),
        "- Use only actions listed in the Session Baseline capability surface.".to_string(),
        "- Use canonical action ids in the format `env__action`.".to_string(),
        "- Provide exact action arguments that match the runtime-enforced schema.".to_string(),
        "- For optional arguments, omit fields you do not need and never send empty placeholder strings.".to_string(),
        String::new(),
        "## Response vs Execution".to_string(),
        "- Prefer the smallest sufficient next move.".to_string(),
        "- If the available evidence is already sufficient, answer the user directly.".to_string(),
        "- If more information is needed, choose the actions that reduce uncertainty most directly.".to_string(),
        "- Do not chain executions reflexively when a direct response is already justified.".to_string(),
        "- Use action execution when the user request requires real inspection, retrieval, or state change.".to_string(),
        "- Do not continue chaining actions for too long without responding to the user.".to_string(),
        "- When you already have a meaningful update, partial answer, blocker, or decision point, respond instead of extending the execution chain.".to_string(),
        "- Use additional actions only when they are still necessary to improve the next response or complete the requested work.".to_string(),
        String::new(),
        "## Execution Rules".to_string(),
        "- Execution requests default to `await`.".to_string(),
        "- Request `detach` only when an action's `mode_support` is `await_or_detach`.".to_string(),
        "- If an action is `await_only`, requesting `detach` will be rejected.".to_string(),
        "- Use `detach` only when the current turn does not need that result to decide the next move.".to_string(),
        "- Multiple executions may be emitted in the same turn.".to_string(),
        String::new(),
        "## Evidence and Payloads".to_string(),
        "- Treat execution previews and transcript events as evidence.".to_string(),
        "- Use Resolved Payload Lookups when present before issuing additional payload fetches.".to_string(),
        "- Prefer previews first and fetch larger payload slices only when they are necessary for the next decision.".to_string(),
        "- Avoid redundant payload fetches when equivalent evidence is already present.".to_string(),
        String::new(),
        "## State Assumptions".to_string(),
        "- Do not assume current time unless an execution result or event provides it explicitly.".to_string(),
        "- Do not assume live environment state unless an execution result or event provides it explicitly.".to_string(),
        "- Treat the Session Baseline as the durable contract for this prompt.".to_string(),
        "- Treat additive events as authoritative updates after the baseline.".to_string(),
        String::new(),
        "## Failure Handling".to_string(),
        "- `execution_rejected` means the runtime did not accept the requested execution; revise the request instead of assuming it ran.".to_string(),
        "- `awaited_execution_failed` and `detached_execution_failed` mean execution was accepted but ended unsuccessfully.".to_string(),
        "- Use the failure message and any payload preview to decide whether to retry, inspect further, change approach, or report failure.".to_string(),
        String::new(),
        "## Response Style".to_string(),
        "- Be direct and useful.".to_string(),
        "- Do not restate the prompt contract unless it is relevant.".to_string(),
        "- Do not describe your capabilities unless the user asks.".to_string(),
        "- Do not over-explain internal execution mechanics unless they matter to the user.".to_string(),
    ]
    .join("\n")
}

pub(super) fn build_identity_envelope_block(input: &PromptInput) -> String {
    let lines = [
        "# Identity Envelope".to_string(),
        format!(
            "- `schema_version`: {}",
            input.stable_prefix.identity_envelope.schema_version
        ),
        format!(
            "- `source_revision`: {}",
            input.stable_prefix.identity_envelope.source_revision
        ),
        String::new(),
        "## Identity Material".to_string(),
        String::new(),
        "```md".to_string(),
        render_identity_material_markdown(&input.stable_prefix.identity_envelope.material),
        "```".to_string(),
    ];
    lines.join("\n")
}

pub(super) fn build_session_baseline_block(input: &PromptInput) -> String {
    let mut lines = vec![
        "# Session Baseline".to_string(),
        "## Session Anchor".to_string(),
        format!(
            "- `session_id`: {}",
            input
                .stable_prefix
                .session_baseline
                .session_anchor
                .session_id
        ),
        format!(
            "- `started_at_unix_ms`: {}",
            input
                .stable_prefix
                .session_baseline
                .session_anchor
                .started_at_unix_ms
        ),
        String::new(),
        "## Capability Surface".to_string(),
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
            lines.push(String::new());
            lines.push(format!("### {} (`{}`)", environment.name, environment.id));
            if !environment.description.trim().is_empty() {
                lines.push(String::new());
                lines.push(environment.description);
            }

            let mut actions = environment.actions;
            actions.sort_by(|a, b| a.action_id.cmp(&b.action_id));
            lines.push(String::new());
            lines.push("#### Actions".to_string());
            if actions.is_empty() {
                lines.push("- _No actions available._".to_string());
            } else {
                for action in actions {
                    let discovery = if action.discovery {
                        " · `discovery`"
                    } else {
                        ""
                    };
                    lines.push(format!(
                        "- `{}` · `{}`{}",
                        action.action_id,
                        action.mode_support.as_str(),
                        discovery
                    ));
                    lines.push(format!("  {}", action.description));
                }
            }

            let mut recipes = environment.recipes;
            recipes.sort_by(|a, b| a.title.cmp(&b.title));
            if !recipes.is_empty() {
                lines.push(String::new());
                lines.push("#### Recipes".to_string());
                for recipe in recipes {
                    lines.push(String::new());
                    lines.push(format!("##### {}", recipe.title));
                    lines.push(String::new());
                    lines.push("```md".to_string());
                    for step in recipe.steps {
                        lines.push(format!("- {}", step));
                    }
                    lines.push("```".to_string());
                }
            }
        }
    }

    lines.push(String::new());
    lines.push("## Participant Envelope".to_string());
    lines.push(format!(
        "- `schema_version`: {}",
        input
            .stable_prefix
            .session_baseline
            .participant_envelope
            .schema_version
    ));
    lines.push(format!(
        "- `source_revision`: {}",
        input
            .stable_prefix
            .session_baseline
            .participant_envelope
            .source_revision
    ));
    lines.push(String::new());
    lines.push("### Participant Material".to_string());
    lines.push(String::new());
    lines.push("```md".to_string());
    lines.push(render_participant_material_markdown(
        &input
            .stable_prefix
            .session_baseline
            .participant_envelope
            .material,
    ));
    lines.push("```".to_string());

    lines.join("\n")
}

fn render_identity_material_markdown(material: &Value) -> String {
    render_markdown_material(material)
}

fn render_participant_material_markdown(material: &Value) -> String {
    let Some(participants) = material
        .as_object()
        .and_then(|object| object.get("participants"))
        .and_then(Value::as_array)
    else {
        return "_No participant material provided._".to_string();
    };

    if participants.is_empty() {
        return "_No participant material provided._".to_string();
    }

    let mut rendered = participants
        .iter()
        .filter_map(Value::as_object)
        .map(|participant| {
            let user_id = participant
                .get("user_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown-user")
                .to_string();
            let mut body = participant.clone();
            body.remove("user_id");
            (user_id, render_markdown_map(&body, 3))
        })
        .collect::<Vec<_>>();
    rendered.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lines = Vec::new();
    for (index, (user_id, body)) in rendered.into_iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(format!("## {user_id}"));
        if !body.trim().is_empty() {
            lines.push(String::new());
            lines.push(body);
        }
    }

    if lines.is_empty() {
        "_No participant material provided._".to_string()
    } else {
        lines.join("\n")
    }
}

fn render_markdown_material(material: &Value) -> String {
    match material {
        Value::Null => "_No material provided._".to_string(),
        Value::String(text) => render_markdown_text(text, "_No material provided._"),
        Value::Object(object) => {
            if let Some(markdown) = object.get("markdown").and_then(Value::as_str) {
                return render_markdown_text(markdown, "_No material provided._");
            }
            let rendered = render_markdown_map(object, 2);
            if rendered.trim().is_empty() {
                "_No material provided._".to_string()
            } else {
                rendered
            }
        }
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn render_markdown_map(map: &Map<String, Value>, heading_level: usize) -> String {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();

    let mut lines = Vec::new();
    for key in keys {
        let value = map.get(&key).expect("map key");
        render_markdown_entry(&mut lines, &key, value, heading_level);
    }
    lines.join("\n")
}

fn render_markdown_entry(lines: &mut Vec<String>, key: &str, value: &Value, heading_level: usize) {
    match value {
        Value::Null => {}
        Value::Object(object) => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!(
                "{} {}",
                "#".repeat(heading_level),
                titleize_key(key)
            ));
            let nested = render_markdown_map(object, heading_level + 1);
            if nested.trim().is_empty() {
                lines.push("_No content provided._".to_string());
            } else {
                lines.push(String::new());
                lines.push(nested);
            }
        }
        Value::Array(items) => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!(
                "{} {}",
                "#".repeat(heading_level),
                titleize_key(key)
            ));
            lines.push(String::new());
            if items.is_empty() {
                lines.push("_No entries provided._".to_string());
                return;
            }

            if items.iter().all(is_scalar_value) {
                for item in items {
                    lines.push(format!("- {}", scalar_to_markdown(item)));
                }
                return;
            }

            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    lines.push(String::new());
                }
                match item {
                    Value::Object(object) => {
                        let label = object
                            .get("title")
                            .or_else(|| object.get("name"))
                            .or_else(|| object.get("id"))
                            .or_else(|| object.get("key"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("Item {}", index + 1));
                        lines.push(format!("{} {}", "#".repeat(heading_level + 1), label));
                        let nested = render_markdown_map(object, heading_level + 2);
                        if !nested.trim().is_empty() {
                            lines.push(String::new());
                            lines.push(nested);
                        }
                    }
                    other => lines.push(format!("- {}", scalar_to_markdown(other))),
                }
            }
        }
        scalar => lines.push(format!("- `{}`: {}", key, scalar_to_markdown(scalar))),
    }
}

fn render_markdown_text(text: &str, empty_placeholder: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        empty_placeholder.to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_scalar_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn scalar_to_markdown(value: &Value) -> String {
    match value {
        Value::Null => "_null_".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.to_string(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}

fn titleize_key(key: &str) -> String {
    key.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
