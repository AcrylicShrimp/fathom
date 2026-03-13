use std::collections::{BTreeMap, BTreeSet};

use crate::agent::types::PromptInput;

use super::render::render_event_transcript_lines;
use super::timeline::{TimelineEvent, TimelineKind};
use super::util::{estimate_tokens, read_ratio_env, read_usize_env};
use super::{
    COMPACTION_BATCH_EVENTS, DEFAULT_CONTEXT_LIMIT_TOKENS, DEFAULT_HARD_CONTEXT_RATIO,
    DEFAULT_SOFT_CONTEXT_RATIO, MAX_SESSION_SUMMARY_BLOCKS_IN_PROMPT,
    MIN_TIMELINE_EVENTS_AFTER_COMPACTION, MIN_TIMELINE_EVENTS_AFTER_HARD_TRIM,
};

pub(super) fn build_session_compaction_summaries(input: &PromptInput) -> (Vec<String>, usize) {
    if input.compaction_blocks.is_empty() {
        return (Vec::new(), 0);
    }

    let mut summary_blocks = input.compaction_blocks.clone();
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

pub(super) fn compact_timeline(
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
            TimelineKind::ExecutionRequested => "execution_requested",
            TimelineKind::ExecutionSucceeded => "execution_succeeded",
            TimelineKind::ExecutionFailed => "execution_failed",
            TimelineKind::ExecutionBackgrounded => "execution_backgrounded",
            TimelineKind::ExecutionCanceled => "execution_canceled",
            TimelineKind::ExecutionRejected => "execution_rejected",
        };
        *counts.entry(key).or_default() += 1;
        if let Some(action) = &event.action_id {
            actions.insert(action.clone());
        }
    }
    let actions_preview = actions.into_iter().take(4).collect::<Vec<_>>().join(",");
    format!(
        "summary_block[{index}] ts=[{first_ts},{last_ts}] events={} user_message={} assistant_output={} execution_requested={} execution_succeeded={} execution_failed={} execution_backgrounded={} execution_canceled={} execution_rejected={} actions=[{}]",
        batch.len(),
        counts.get("user_message").copied().unwrap_or_default(),
        counts.get("assistant_output").copied().unwrap_or_default(),
        counts
            .get("execution_requested")
            .copied()
            .unwrap_or_default(),
        counts
            .get("execution_succeeded")
            .copied()
            .unwrap_or_default(),
        counts.get("execution_failed").copied().unwrap_or_default(),
        counts
            .get("execution_backgrounded")
            .copied()
            .unwrap_or_default(),
        counts
            .get("execution_canceled")
            .copied()
            .unwrap_or_default(),
        counts
            .get("execution_rejected")
            .copied()
            .unwrap_or_default(),
        actions_preview
    )
}

fn estimate_timeline_tokens(summaries: &[String], events: &[TimelineEvent]) -> usize {
    let lines = render_event_transcript_lines(summaries, events);
    estimate_tokens(&lines.join("\n"))
}
