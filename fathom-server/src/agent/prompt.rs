use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::hash::{Hash, Hasher};

use crate::agent::types::{PromptMessage, PromptMessageBundle, PromptMessageStat, TurnSnapshot};
use crate::history::{HistoryEvent, HistoryEventKind, PayloadPreview};
use crate::history::{TASK_PAYLOAD_LOOKUP_ACTION, build_payload_preview};
use crate::pb;
use crate::util::task_status_label;

const TOKEN_DIVISOR_CHARS: usize = 4;
const DEFAULT_CONTEXT_LIMIT_TOKENS: usize = 128_000;
const DEFAULT_SOFT_CONTEXT_RATIO: f64 = 0.70;
const DEFAULT_HARD_CONTEXT_RATIO: f64 = 0.85;
const TIMELINE_SECTION_MAX_TOKENS: usize = 2_500;
const MIN_TIMELINE_EVENTS_AFTER_COMPACTION: usize = 18;
const MIN_TIMELINE_EVENTS_AFTER_HARD_TRIM: usize = 8;
const COMPACTION_BATCH_EVENTS: usize = 12;
const MAX_SESSION_SUMMARY_BLOCKS_IN_PROMPT: usize = 8;
const MAX_INLINE_TEXT_CHARS: usize = 320;
const MAX_PREVIEW_HEAD_CHARS: usize = 180;
const MAX_PREVIEW_TAIL_CHARS: usize = 120;
const MAX_LOOKUP_PAYLOAD_CHARS: usize = 1_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TimelineKind {
    UserMessage,
    AssistantOutput,
    ActionStarted,
    ActionFinished,
}

#[derive(Debug, Clone)]
struct TimelineEvent {
    ts_unix_ms: i64,
    seq: usize,
    kind: TimelineKind,
    task_id: Option<String>,
    action_id: Option<String>,
    status: Option<String>,
    line: String,
}

#[derive(Debug, Clone, Default)]
struct TimelineBuild {
    events: Vec<TimelineEvent>,
    raw_events: usize,
    dedup_dropped: usize,
}

pub(crate) fn build_agent_prompt_bundle(
    snapshot: &TurnSnapshot,
    retry_feedback: Option<&str>,
) -> PromptMessageBundle {
    let harness_contract = build_harness_contract_block(snapshot);
    let identity_envelope = build_identity_envelope_block(snapshot);
    let session_baseline = build_session_baseline_block(snapshot);
    let tail_event_lines = build_tail_event_lines(snapshot, retry_feedback);

    let timeline = build_canonical_timeline(snapshot);
    let (session_summary_lines, session_summary_count) =
        build_session_compaction_summaries(snapshot);

    let non_timeline_estimated = estimate_tokens(&harness_contract)
        + estimate_tokens(&identity_envelope)
        + estimate_tokens(&session_baseline)
        + estimate_tokens(&tail_event_lines.join("\n"));
    let (timeline_events, summary_lines, compaction_reason, compacted_events) = compact_timeline(
        &timeline.events,
        &session_summary_lines,
        session_summary_count,
        non_timeline_estimated,
    );

    let event_lines =
        render_event_transcript_lines(&summary_lines, &timeline_events, &tail_event_lines);
    let event_messages = chunk_section_messages(
        "event_transcript",
        "## Event Transcript",
        &event_lines,
        TIMELINE_SECTION_MAX_TOKENS,
    );

    let mut bundle = PromptMessageBundle::default();
    push_message(
        &mut bundle,
        "system",
        "harness_contract",
        harness_contract,
        estimate_tokens,
    );
    push_message(
        &mut bundle,
        "system",
        "identity_envelope",
        identity_envelope,
        estimate_tokens,
    );
    push_message(
        &mut bundle,
        "system",
        "session_baseline",
        session_baseline,
        estimate_tokens,
    );
    for (label, content) in event_messages {
        push_message(&mut bundle, "user", &label, content, estimate_tokens);
    }

    bundle.stats.timeline_raw_events = timeline.raw_events;
    bundle.stats.timeline_compacted_events = compacted_events;
    bundle.stats.dedup_dropped_events = timeline.dedup_dropped;
    bundle.stats.compaction_applied = !summary_lines.is_empty() || compacted_events > 0;
    bundle.stats.compaction_reason = compaction_reason;
    bundle.stats.messages_count = bundle.messages.len();
    bundle.stats.estimated_prompt_tokens = bundle
        .stats
        .per_message
        .iter()
        .map(|item| item.estimated_tokens)
        .sum::<usize>();
    bundle.stats.stable_prefix_hash = stable_prefix_hash(&bundle.messages);
    bundle
}

fn push_message<F>(
    bundle: &mut PromptMessageBundle,
    role: &str,
    label: &str,
    content: String,
    token_estimator: F,
) where
    F: Fn(&str) -> usize,
{
    let message = PromptMessage::new(role.to_string(), label.to_string(), content);
    let stat = PromptMessageStat {
        label: label.to_string(),
        role: role.to_string(),
        estimated_tokens: token_estimator(&message.content),
        stable_hash: message.stable_hash.clone(),
    };
    bundle.messages.push(message);
    bundle.stats.per_message.push(stat);
}

fn build_harness_contract_block(snapshot: &TurnSnapshot) -> String {
    [
        "## Harness Contract".to_string(),
        format!(
            "runtime_version: {}",
            snapshot.harness_contract.runtime_version
        ),
        format!(
            "contract_schema_version: {}",
            snapshot.harness_contract.contract_schema_version
        ),
        String::new(),
        "You may emit assistant text and/or tool calls in the same turn.".to_string(),
        "Use only tools listed in the Session Baseline capability surface.".to_string(),
        "Use canonical tool names in the format env__action.".to_string(),
        "Every tool call must include a concise `reasoning` field that explains why the call is necessary now.".to_string(),
        "Tool calls default to `await` semantics.".to_string(),
        "Request `detach` only when a tool's `mode_support` is `await_or_detach`.".to_string(),
        "If a tool is `await_only`, requesting `detach` will be rejected.".to_string(),
        "Use Resolved Payload Lookups when present before issuing additional payload fetches.".to_string(),
        "Do not assume current time unless a tool result or event provides it explicitly.".to_string(),
        "Do not assume live environment state unless a tool result or event provides it explicitly.".to_string(),
        "Action input schemas are enforced by the runtime; provide exact argument shapes.".to_string(),
        "For optional action arguments, omit fields you do not need; never send empty placeholder strings.".to_string(),
        "Avoid unbounded tool chaining. When evidence is sufficient, provide a direct assistant report to the user.".to_string(),
    ]
    .join("\n")
}

fn build_identity_envelope_block(snapshot: &TurnSnapshot) -> String {
    let lines = [
        "## Identity Envelope".to_string(),
        format!(
            "schema_version: {}",
            snapshot.identity_envelope.schema_version
        ),
        format!(
            "source_revision: {}",
            snapshot.identity_envelope.source_revision
        ),
        "material_json:".to_string(),
        serialize_pretty_json(&snapshot.identity_envelope.material),
    ];
    lines.join("\n")
}

fn build_session_baseline_block(snapshot: &TurnSnapshot) -> String {
    let mut lines = vec![
        "## Session Baseline".to_string(),
        "### Session Anchor".to_string(),
        format!(
            "session_id: {}",
            snapshot.session_baseline.session_anchor.session_id
        ),
        format!(
            "started_at_unix_ms: {}",
            snapshot.session_baseline.session_anchor.started_at_unix_ms
        ),
        String::new(),
        "### Capability Surface".to_string(),
    ];
    let mut environments = snapshot
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
            let mut tools = environment.tools;
            tools.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
            if tools.is_empty() {
                lines.push("  tools: (none)".to_string());
            } else {
                lines.push("  tools:".to_string());
                for tool in tools {
                    if tool.discovery {
                        lines.push(format!(
                            "  - {} [{}] (discovery): {}",
                            tool.tool_name,
                            tool.mode_support.as_str(),
                            tool.description
                        ));
                    } else {
                        lines.push(format!(
                            "  - {} [{}]: {}",
                            tool.tool_name,
                            tool.mode_support.as_str(),
                            tool.description
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
        snapshot
            .session_baseline
            .participant_envelope
            .schema_version
    ));
    lines.push(format!(
        "source_revision: {}",
        snapshot
            .session_baseline
            .participant_envelope
            .source_revision
    ));
    lines.push("material_json:".to_string());
    lines.push(serialize_pretty_json(
        &snapshot.session_baseline.participant_envelope.material,
    ));

    lines.join("\n")
}

fn build_tail_event_lines(snapshot: &TurnSnapshot, retry_feedback: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();

    for trigger in &snapshot.triggers {
        lines.push(format!("pending_trigger {}", trigger_text_compact(trigger)));
    }
    lines.extend(build_resolved_lookup_event_lines(snapshot));

    if let Some(feedback) = retry_feedback {
        lines.push(format!(
            "retry_feedback {}",
            truncate_inline(feedback, MAX_LOOKUP_PAYLOAD_CHARS)
        ));
    }

    if lines.is_empty() {
        lines.push("event_log_empty".to_string());
    }

    lines
}

fn build_resolved_lookup_event_lines(snapshot: &TurnSnapshot) -> Vec<String> {
    if snapshot.resolved_payload_lookups.is_empty() {
        return Vec::new();
    }

    let mut dedup =
        BTreeMap::<(String, String, usize), crate::agent::ResolvedPayloadLookupHint>::new();
    for lookup in &snapshot.resolved_payload_lookups {
        dedup.insert(
            (lookup.task_id.clone(), lookup.part.clone(), lookup.offset),
            lookup.clone(),
        );
    }

    let mut lines = Vec::new();
    for (_, lookup) in dedup {
        lines.push(format!(
            "resolved_payload_lookup lookup_task_id={} task_id={} part={} offset={} next_offset={} full_bytes={} source_truncated={} injected_truncated={} injected_omitted_bytes={}",
            lookup.lookup_task_id,
            lookup.task_id,
            lookup.part,
            lookup.offset,
            lookup
                .next_offset
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-1".to_string()),
            lookup.full_bytes,
            lookup.source_truncated,
            lookup.injected_truncated,
            lookup.injected_omitted_bytes
        ));
        lines.push(format!(
            "payload_chunk {}",
            truncate_inline(&lookup.payload_chunk, MAX_LOOKUP_PAYLOAD_CHARS)
        ));
    }
    lines
}

fn build_session_compaction_summaries(snapshot: &TurnSnapshot) -> (Vec<String>, usize) {
    if snapshot.compaction.summary_blocks.is_empty() {
        return (Vec::new(), 0);
    }

    let mut summary_blocks = snapshot.compaction.summary_blocks.clone();
    summary_blocks.sort_by(|a, b| {
        a.source_range_start
            .cmp(&b.source_range_start)
            .then(a.source_range_end.cmp(&b.source_range_end))
    });

    let omitted = summary_blocks
        .len()
        .saturating_sub(MAX_SESSION_SUMMARY_BLOCKS_IN_PROMPT);
    let total_blocks = summary_blocks.len();
    let retained = summary_blocks
        .into_iter()
        .skip(omitted)
        .map(|block| block.summary_text)
        .collect::<Vec<_>>();

    let mut lines = Vec::new();
    if omitted > 0 {
        lines.push(format!(
            "... {} older session summary block(s) omitted ...",
            omitted
        ));
    }
    lines.extend(retained);
    (lines, total_blocks)
}

fn build_canonical_timeline(snapshot: &TurnSnapshot) -> TimelineBuild {
    let mut raw = Vec::<TimelineEvent>::new();
    let mut dedup_dropped = 0usize;
    let mut raw_events = 0usize;
    let mut finished_task_ids = HashSet::<String>::new();

    for event in &snapshot.recent_history {
        if matches!(&event.kind, HistoryEventKind::TaskFinished(_)) {
            finished_task_ids.insert(event.actor_id.clone());
        }
    }

    for (seq, event) in snapshot.recent_history.iter().enumerate() {
        match &event.kind {
            HistoryEventKind::TriggerTaskDone(_) => {
                let task_id = event.actor_id.clone();
                if finished_task_ids.contains(&task_id) {
                    dedup_dropped += 1;
                    continue;
                }
                if let Some(event) = timeline_event_from_task_done_event(event, seq) {
                    raw_events += 1;
                    raw.push(event);
                }
            }
            _ => {
                if let Some(event) = timeline_event_from_history_event(event, seq) {
                    raw_events += 1;
                    raw.push(event);
                }
            }
        }
    }

    raw.sort_by(|a, b| a.ts_unix_ms.cmp(&b.ts_unix_ms).then(a.seq.cmp(&b.seq)));

    let mut action_started_seen = HashSet::<String>::new();
    let mut action_finished_seen = HashSet::<String>::new();
    let mut events = Vec::with_capacity(raw.len());
    for event in raw {
        match event.kind {
            TimelineKind::ActionStarted => {
                let Some(task_id) = event.task_id.clone() else {
                    events.push(event);
                    continue;
                };
                if !action_started_seen.insert(task_id) {
                    dedup_dropped += 1;
                    continue;
                }
            }
            TimelineKind::ActionFinished => {
                let key = format!(
                    "{}:{}:{}",
                    event.task_id.clone().unwrap_or_default(),
                    event.status.clone().unwrap_or_default(),
                    event.action_id.clone().unwrap_or_default()
                );
                if !action_finished_seen.insert(key) {
                    dedup_dropped += 1;
                    continue;
                }
            }
            _ => {}
        }
        events.push(event);
    }

    TimelineBuild {
        events,
        raw_events,
        dedup_dropped,
    }
}

fn timeline_event_from_task_done_event(event: &HistoryEvent, seq: usize) -> Option<TimelineEvent> {
    let HistoryEventKind::TriggerTaskDone(payload) = &event.kind else {
        return None;
    };
    let task_id = event.actor_id.clone();
    let status = payload.status.clone();
    let result_preview = preview_to_inline(&payload.result_preview);
    Some(TimelineEvent {
        ts_unix_ms: event.ts_unix_ms,
        seq,
        kind: TimelineKind::ActionFinished,
        task_id: Some(task_id.clone()),
        action_id: Some("unknown".to_string()),
        status: Some(status.clone()),
        line: format!(
            "action_finished task_id={} action=unknown env=unknown status={} result_preview={} source=trigger_task_done",
            task_id, status, result_preview
        ),
    })
}

fn timeline_event_from_history_event(event: &HistoryEvent, seq: usize) -> Option<TimelineEvent> {
    match &event.kind {
        HistoryEventKind::TriggerUserMessage(payload) => {
            let text = truncate_inline(&payload.text, MAX_INLINE_TEXT_CHARS);
            Some(TimelineEvent {
                ts_unix_ms: event.ts_unix_ms,
                seq,
                kind: TimelineKind::UserMessage,
                task_id: None,
                action_id: None,
                status: None,
                line: format!("user_message user={} text={}", event.actor_id, text),
            })
        }
        HistoryEventKind::AssistantOutput(payload) => {
            let content = truncate_inline(&payload.content, MAX_INLINE_TEXT_CHARS);
            Some(TimelineEvent {
                ts_unix_ms: event.ts_unix_ms,
                seq,
                kind: TimelineKind::AssistantOutput,
                task_id: None,
                action_id: None,
                status: None,
                line: format!("assistant_output content={content}"),
            })
        }
        HistoryEventKind::TaskStarted(payload) => {
            let task_id = event.actor_id.clone();
            let action_id = payload.canonical_action_id.clone();
            let env_id = payload.environment_id.as_str();
            let status = payload.status.clone();
            let args_preview = preview_to_inline(&payload.args_preview);
            Some(TimelineEvent {
                ts_unix_ms: event.ts_unix_ms,
                seq,
                kind: TimelineKind::ActionStarted,
                task_id: Some(task_id.clone()),
                action_id: Some(action_id.clone()),
                status: Some(status.clone()),
                line: format!(
                    "action_started task_id={} action={} env={} status={} args_preview={}",
                    task_id, action_id, env_id, status, args_preview
                ),
            })
        }
        HistoryEventKind::TaskFinished(payload) => {
            let task_id = event.actor_id.clone();
            let action_id = payload.canonical_action_id.clone();
            let env_id = payload.environment_id.as_str();
            let status = payload.status.clone();
            let result_preview = preview_to_inline(&payload.result_preview);
            Some(TimelineEvent {
                ts_unix_ms: event.ts_unix_ms,
                seq,
                kind: TimelineKind::ActionFinished,
                task_id: Some(task_id.clone()),
                action_id: Some(action_id.clone()),
                status: Some(status.clone()),
                line: format!(
                    "action_finished task_id={} action={} env={} status={} result_preview={}",
                    task_id, action_id, env_id, status, result_preview
                ),
            })
        }
        HistoryEventKind::TriggerTaskDone(_)
        | HistoryEventKind::TriggerUnknown
        | HistoryEventKind::TriggerHeartbeat
        | HistoryEventKind::TriggerCron(_)
        | HistoryEventKind::TriggerRefreshProfile(_) => None,
    }
}

fn compact_timeline(
    timeline: &[TimelineEvent],
    initial_summaries: &[String],
    session_summary_count: usize,
    non_timeline_tokens: usize,
) -> (Vec<TimelineEvent>, Vec<String>, String, usize) {
    let mut remaining = timeline.to_vec();
    let mut summaries = initial_summaries.to_vec();
    let mut compacted_count = 0usize;

    let context_limit = read_usize_env(
        "FATHOM_AGENT_CONTEXT_LIMIT_TOKENS",
        DEFAULT_CONTEXT_LIMIT_TOKENS,
    );
    let soft_ratio = read_ratio_env(
        "FATHOM_AGENT_CONTEXT_SOFT_RATIO",
        DEFAULT_SOFT_CONTEXT_RATIO,
    );
    let hard_ratio = read_ratio_env(
        "FATHOM_AGENT_CONTEXT_HARD_RATIO",
        DEFAULT_HARD_CONTEXT_RATIO,
    );
    let soft_limit = (context_limit as f64 * soft_ratio).round() as usize;
    let hard_limit = (context_limit as f64 * hard_ratio).round() as usize;

    while non_timeline_tokens + estimate_timeline_tokens(&summaries, &remaining) > soft_limit
        && remaining.len() > MIN_TIMELINE_EVENTS_AFTER_COMPACTION
    {
        let compactable = remaining
            .len()
            .saturating_sub(MIN_TIMELINE_EVENTS_AFTER_COMPACTION);
        let batch = compactable.min(COMPACTION_BATCH_EVENTS);
        if batch == 0 {
            break;
        }
        let drained = remaining.drain(0..batch).collect::<Vec<_>>();
        compacted_count += drained.len();
        summaries.push(summarize_timeline_batch(
            summaries.len().saturating_add(1),
            &drained,
        ));
    }

    while non_timeline_tokens + estimate_timeline_tokens(&summaries, &remaining) > hard_limit
        && remaining.len() > MIN_TIMELINE_EVENTS_AFTER_HARD_TRIM
    {
        remaining.remove(0);
        compacted_count += 1;
    }

    let reason = build_compaction_reason(session_summary_count, compacted_count);
    (remaining, summaries, reason, compacted_count)
}

fn build_compaction_reason(session_summary_count: usize, compacted_count: usize) -> String {
    if session_summary_count == 0 && compacted_count == 0 {
        return "none".to_string();
    }

    let mut parts = Vec::new();
    if session_summary_count > 0 {
        parts.push(format!("session_summary_blocks={session_summary_count}"));
    }
    if compacted_count > 0 {
        parts.push(format!(
            "prompt_soft_compaction compacted_events={compacted_count}"
        ));
    }
    parts.join(" + ")
}

fn summarize_timeline_batch(index: usize, batch: &[TimelineEvent]) -> String {
    if batch.is_empty() {
        return format!("summary_block[{index}] (empty)");
    }
    let mut counts = BTreeMap::<&'static str, usize>::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut actions = BTreeSet::<String>::new();
    let first_ts = batch
        .first()
        .map(|item| item.ts_unix_ms)
        .unwrap_or_default();
    let last_ts = batch.last().map(|item| item.ts_unix_ms).unwrap_or_default();

    for event in batch {
        let key = match event.kind {
            TimelineKind::UserMessage => "user_message",
            TimelineKind::AssistantOutput => "assistant_output",
            TimelineKind::ActionStarted => "action_started",
            TimelineKind::ActionFinished => "action_finished",
        };
        *counts.entry(key).or_default() += 1;
        if let Some(status) = &event.status {
            *statuses.entry(status.clone()).or_default() += 1;
        }
        if let Some(action) = &event.action_id {
            actions.insert(action.clone());
        }
    }
    let actions_preview = actions.into_iter().take(4).collect::<Vec<_>>().join(",");
    let status_preview = statuses
        .into_iter()
        .map(|(status, count)| format!("{status}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "summary_block[{index}] ts=[{first_ts},{last_ts}] events={} user_message={} assistant_output={} action_started={} action_finished={} statuses=[{}] actions=[{}]",
        batch.len(),
        counts.get("user_message").copied().unwrap_or_default(),
        counts.get("assistant_output").copied().unwrap_or_default(),
        counts.get("action_started").copied().unwrap_or_default(),
        counts.get("action_finished").copied().unwrap_or_default(),
        status_preview,
        actions_preview
    )
}

fn render_event_transcript_lines(
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

fn estimate_timeline_tokens(summaries: &[String], events: &[TimelineEvent]) -> usize {
    let lines = render_event_transcript_lines(summaries, events, &[]);
    estimate_tokens(&lines.join("\n"))
}

fn chunk_section_messages(
    base_label: &str,
    heading: &str,
    lines: &[String],
    max_tokens: usize,
) -> Vec<(String, String)> {
    let safe_max_tokens = max_tokens.max(256);
    let mut chunks = Vec::<Vec<String>>::new();
    let mut current = vec![heading.to_string()];
    let mut current_tokens = estimate_tokens(heading);
    let continuation_heading = format!("{heading} (continued)");

    let normalized_lines = if lines.is_empty() {
        vec!["(none)".to_string()]
    } else {
        lines.to_vec()
    };

    for line in normalized_lines {
        let line_tokens = estimate_tokens(&line);
        if current.len() > 1 && current_tokens + line_tokens > safe_max_tokens {
            chunks.push(current);
            current = vec![continuation_heading.clone()];
            current_tokens = estimate_tokens(&continuation_heading);
        }
        current.push(line);
        current_tokens += line_tokens;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            let label = if index == 0 {
                base_label.to_string()
            } else {
                format!("{base_label}.{}", index + 1)
            };
            (label, chunk.join("\n"))
        })
        .collect()
}

fn trigger_text_compact(trigger: &pb::Trigger) -> String {
    let Some(kind) = trigger.kind.as_ref() else {
        return "unknown_trigger".to_string();
    };

    match kind {
        pb::trigger::Kind::UserMessage(message) => {
            format!(
                "user_message user={} text={}",
                message.user_id,
                truncate_inline(&message.text, MAX_INLINE_TEXT_CHARS)
            )
        }
        pb::trigger::Kind::TaskDone(done) => {
            let status = pb::TaskStatus::try_from(done.status)
                .map(task_status_label)
                .unwrap_or("unknown");
            let preview = build_payload_preview(
                &done.result_message,
                format!("task://{}/result", done.task_id),
            );
            let preview_text = format!(
                "lookup_ref={} full_bytes={} omitted_bytes={} truncated={} head={} tail={}",
                preview.lookup_ref,
                preview.full_bytes,
                preview.omitted_bytes,
                preview.truncated,
                truncate_inline(&preview.head, MAX_PREVIEW_HEAD_CHARS),
                truncate_inline(&preview.tail, MAX_PREVIEW_TAIL_CHARS),
            );
            format!(
                "task_done task_id={} status={} result_preview={} lookup_action={}",
                done.task_id, status, preview_text, TASK_PAYLOAD_LOOKUP_ACTION
            )
        }
        pb::trigger::Kind::Heartbeat(_) => "heartbeat".to_string(),
        pb::trigger::Kind::Cron(cron) => format!("cron key={}", cron.key),
        pb::trigger::Kind::RefreshProfile(refresh) => {
            format!(
                "refresh_profile scope={} user_id={}",
                refresh.scope, refresh.user_id
            )
        }
    }
}

fn preview_to_inline(preview: &PayloadPreview) -> String {
    let head = truncate_inline(&preview.head, MAX_PREVIEW_HEAD_CHARS);
    let tail = truncate_inline(&preview.tail, MAX_PREVIEW_TAIL_CHARS);
    format!(
        "lookup_ref={} full_bytes={} omitted_bytes={} truncated={} head={} tail={}",
        preview.lookup_ref,
        preview.full_bytes,
        preview.omitted_bytes,
        preview.truncated,
        head,
        tail
    )
}

fn stable_prefix_hash(messages: &[PromptMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for message in messages.iter().take(3) {
        message.stable_hash.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count().saturating_add(TOKEN_DIVISOR_CHARS - 1)) / TOKEN_DIVISOR_CHARS
}

fn truncate_inline(input: &str, max_chars: usize) -> String {
    let sanitized = input.replace('\n', "\\n").replace('\r', "\\r");
    let total = sanitized.chars().count();
    if total <= max_chars {
        return sanitized;
    }
    let prefix = sanitized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    format!("{prefix}...")
}

fn serialize_pretty_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

fn read_usize_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_ratio_env(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .filter(|value| *value > 0.0 && *value <= 1.0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use crate::agent::{
        CapabilityEnvironmentSnapshot, CapabilityRecipeSnapshot, CapabilitySurfaceSnapshot,
        CapabilityToolSnapshot, HarnessContractSnapshot, IdentityEnvelopeSnapshot,
        ParticipantEnvelopeSnapshot, ResolvedPayloadLookupHint, SessionAnchorSnapshot,
        SessionBaselineSnapshot, SessionCompactionSnapshot, SummaryBlockRefSnapshot,
        ToolModeSupport, TurnSnapshot,
    };
    use crate::history::HistoryEvent;
    use crate::history::PayloadPreview;
    use crate::history::schema::{
        HistoryActorKind, HistoryEventKind, TaskDoneHistoryPayload, TaskFinishedHistoryPayload,
        UserMessageHistoryPayload,
    };
    use crate::pb;
    use crate::util::default_agent_profile;
    use serde_json::json;

    use super::build_agent_prompt_bundle;

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

    fn base_snapshot(recent_history: Vec<HistoryEvent>) -> TurnSnapshot {
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
                    environments: vec![
                        CapabilityEnvironmentSnapshot {
                            id: "filesystem".to_string(),
                            name: "Filesystem".to_string(),
                            description: "Stateful filesystem environment rooted at a base path."
                                .to_string(),
                            tools: vec![CapabilityToolSnapshot {
                                tool_name: "filesystem__list".to_string(),
                                description:
                                    "List directory entries for a non-empty relative path."
                                        .to_string(),
                                mode_support: ToolModeSupport::AwaitOnly,
                                discovery: false,
                            }],
                            recipes: vec![CapabilityRecipeSnapshot {
                                title: "Find files".to_string(),
                                steps: vec![
                                    "Call filesystem__list with path '.'.".to_string(),
                                    "Call filesystem__read for selected files.".to_string(),
                                ],
                            }],
                        },
                        CapabilityEnvironmentSnapshot {
                            id: "system".to_string(),
                            name: "System".to_string(),
                            description: "Inspect runtime context and metadata.".to_string(),
                            tools: vec![CapabilityToolSnapshot {
                                tool_name: "system__get_time".to_string(),
                                description: "Get current server time context.".to_string(),
                                mode_support: ToolModeSupport::AwaitOnly,
                                discovery: true,
                            }],
                            recipes: vec![],
                        },
                    ],
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
            in_flight_actions: vec![],
            resolved_payload_lookups: vec![],
            triggers: vec![],
            recent_history,
            compaction: SessionCompactionSnapshot::default(),
        }
    }

    #[test]
    fn bundle_contains_layered_messages_and_stats() {
        let snapshot = base_snapshot(vec![]);

        let bundle = build_agent_prompt_bundle(&snapshot, None);
        assert!(bundle.messages.len() >= 4);
        assert_eq!(bundle.stats.messages_count, bundle.messages.len());
        assert!(bundle.stats.estimated_prompt_tokens > 0);
        assert!(!bundle.stats.stable_prefix_hash.is_empty());

        let debug_prompt = bundle.as_debug_prompt();
        assert!(debug_prompt.contains("## Harness Contract"));
        assert!(debug_prompt.contains("## Identity Envelope"));
        assert!(debug_prompt.contains("## Session Baseline"));
        assert!(debug_prompt.contains("filesystem__list [await_only]: List directory entries"));
        assert!(debug_prompt.contains(
            "system__get_time [await_only] (discovery): Get current server time context."
        ));
        assert!(debug_prompt.contains(
            "For optional action arguments, omit fields you do not need; never send empty placeholder strings."
        ));
        assert!(!debug_prompt.contains("## Time Context"));
        assert!(debug_prompt.contains("## Event Transcript"));
        assert!(!debug_prompt.contains("## Turn Input"));
        assert!(!debug_prompt.contains("## Resolved Payload Lookups"));
    }

    #[test]
    fn stable_prefix_hash_is_unchanged_by_turn_tail_changes() {
        let snapshot = base_snapshot(vec![]);
        let bundle = build_agent_prompt_bundle(&snapshot, None);

        let mut next_turn = snapshot.clone();
        next_turn.turn_id = 2;
        next_turn.triggers = vec![pb::Trigger {
            trigger_id: "trigger-1".to_string(),
            created_at_unix_ms: 1_765_000_000_100,
            kind: Some(pb::trigger::Kind::UserMessage(pb::UserMessageTrigger {
                user_id: "user-default".to_string(),
                text: "what changed?".to_string(),
            })),
        }];
        next_turn.resolved_payload_lookups = vec![ResolvedPayloadLookupHint {
            lookup_task_id: "lookup-1".to_string(),
            task_id: "task-1".to_string(),
            part: "result".to_string(),
            offset: 0,
            next_offset: Some(120),
            full_bytes: 1024,
            source_truncated: true,
            payload_chunk: "{\"ok\":true}".to_string(),
            injected_truncated: false,
            injected_omitted_bytes: 0,
        }];
        let next_bundle = build_agent_prompt_bundle(&next_turn, Some("retry with better args"));

        assert_eq!(
            bundle.stats.stable_prefix_hash,
            next_bundle.stats.stable_prefix_hash
        );
    }

    #[test]
    fn event_transcript_includes_pending_triggers_and_lookup_availability() {
        let mut snapshot = base_snapshot(vec![]);
        snapshot.triggers = vec![pb::Trigger {
            trigger_id: "trigger-1".to_string(),
            created_at_unix_ms: 1_765_000_000_100,
            kind: Some(pb::trigger::Kind::UserMessage(pb::UserMessageTrigger {
                user_id: "user-default".to_string(),
                text: "inspect the payload".to_string(),
            })),
        }];
        snapshot.resolved_payload_lookups = vec![ResolvedPayloadLookupHint {
            lookup_task_id: "lookup-1".to_string(),
            task_id: "task-1".to_string(),
            part: "result".to_string(),
            offset: 0,
            next_offset: Some(120),
            full_bytes: 1024,
            source_truncated: true,
            payload_chunk: "{\"ok\":true}".to_string(),
            injected_truncated: false,
            injected_omitted_bytes: 0,
        }];

        let bundle = build_agent_prompt_bundle(&snapshot, None);
        let debug_prompt = bundle.as_debug_prompt();

        assert!(
            debug_prompt.contains(
                "pending_trigger user_message user=user-default text=inspect the payload"
            )
        );
        assert!(debug_prompt.contains(
            "resolved_payload_lookup lookup_task_id=lookup-1 task_id=task-1 part=result offset=0"
        ));
        assert!(debug_prompt.contains("payload_chunk {\"ok\":true}"));
    }

    #[test]
    fn typed_history_drives_timeline_without_trigger_task_done_duplicates() {
        let snapshot = base_snapshot(vec![
            HistoryEvent {
                ts_unix_ms: 10,
                actor_kind: HistoryActorKind::User,
                actor_id: "user-default".to_string(),
                profile_ref: "user:user-default@t0".to_string(),
                kind: HistoryEventKind::TriggerUserMessage(UserMessageHistoryPayload {
                    text: "show me the repo files".to_string(),
                }),
            },
            HistoryEvent {
                ts_unix_ms: 20,
                actor_kind: HistoryActorKind::Task,
                actor_id: "task-1".to_string(),
                profile_ref: "agent:agent-default@v1".to_string(),
                kind: HistoryEventKind::TriggerTaskDone(TaskDoneHistoryPayload {
                    status: "succeeded".to_string(),
                    result_preview: sample_preview("task://task-1/result"),
                    lookup_action: "system__get_task_payload".to_string(),
                }),
            },
            HistoryEvent {
                ts_unix_ms: 21,
                actor_kind: HistoryActorKind::Task,
                actor_id: "task-1".to_string(),
                profile_ref: "agent:agent-default@v1".to_string(),
                kind: HistoryEventKind::TaskFinished(TaskFinishedHistoryPayload {
                    canonical_action_id: "filesystem__list".to_string(),
                    environment_id: "filesystem".to_string(),
                    action_name: "list".to_string(),
                    status: "succeeded".to_string(),
                    result_preview: sample_preview("task://task-1/result"),
                    lookup_action: "system__get_task_payload".to_string(),
                }),
            },
        ]);

        let bundle = build_agent_prompt_bundle(&snapshot, None);
        let debug_prompt = bundle.as_debug_prompt();

        assert!(
            debug_prompt.contains("user_message user=user-default text=show me the repo files")
        );
        assert!(debug_prompt.contains(
            "action_finished task_id=task-1 action=filesystem__list env=filesystem status=succeeded"
        ));
        assert!(!debug_prompt.contains("source=trigger_task_done"));
    }

    #[test]
    fn bundle_includes_session_compaction_summaries() {
        let mut snapshot = base_snapshot(vec![]);
        snapshot.compaction = SessionCompactionSnapshot {
            last_compacted_history_index: 24,
            summary_blocks: vec![SummaryBlockRefSnapshot {
                id: "history-summary-000024".to_string(),
                source_range_start: 0,
                source_range_end: 24,
                summary_text: "history-summary-000024 source=[0,24) events=24 user_message=3 assistant_output=2 task_started=4 task_finished=4 task_done=4 refresh_profile=1 heartbeat=0 cron=0 statuses=[succeeded:4] actions=[filesystem__list] users=[user-default]".to_string(),
                created_at_unix_ms: 1_765_000_000_000,
            }],
        };

        let bundle = build_agent_prompt_bundle(&snapshot, None);
        let debug_prompt = bundle.as_debug_prompt();

        assert!(debug_prompt.contains("history-summary-000024 source=[0,24)"));
        assert!(bundle.stats.compaction_applied);
        assert!(
            bundle
                .stats
                .compaction_reason
                .contains("session_summary_blocks=1")
        );
    }
}
