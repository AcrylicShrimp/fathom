use crate::agent::types::{PromptEvent, PromptInput};

use super::MAX_INLINE_TEXT_CHARS;
use super::util::{preview_to_inline, truncate_inline};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TimelineKind {
    UserMessage,
    AssistantOutput,
    ExecutionRequested,
    ExecutionSucceeded,
    ExecutionFailed,
    ExecutionBackgrounded,
    ExecutionCanceled,
    ExecutionRejected,
}

#[derive(Debug, Clone)]
pub(super) struct TimelineEvent {
    pub(super) ts_unix_ms: i64,
    pub(super) seq: usize,
    pub(super) kind: TimelineKind,
    pub(super) action_id: Option<String>,
    pub(super) line: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct TimelineBuild {
    pub(super) events: Vec<TimelineEvent>,
    pub(super) raw_events: usize,
    pub(super) dedup_dropped: usize,
}

pub(super) fn build_canonical_timeline(input: &PromptInput) -> TimelineBuild {
    let mut raw = Vec::<TimelineEvent>::new();
    let mut raw_events = 0usize;

    for (seq, event) in input.transcript_events.iter().enumerate() {
        if let Some(event) = timeline_event_from_prompt_event(event, seq) {
            raw_events += 1;
            raw.push(event);
        }
    }

    raw.sort_by(|a, b| a.ts_unix_ms.cmp(&b.ts_unix_ms).then(a.seq.cmp(&b.seq)));

    TimelineBuild {
        events: raw,
        raw_events,
        dedup_dropped: 0,
    }
}

fn timeline_event_from_prompt_event(event: &PromptEvent, seq: usize) -> Option<TimelineEvent> {
    match event {
        PromptEvent::UserMessage(payload) => {
            let text = truncate_inline(&payload.text, MAX_INLINE_TEXT_CHARS);
            Some(TimelineEvent {
                ts_unix_ms: seq as i64,
                seq,
                kind: TimelineKind::UserMessage,
                action_id: None,
                line: format!("user_message user={} text={}", payload.user_id, text),
            })
        }
        PromptEvent::AssistantOutput(payload) => {
            let content = truncate_inline(&payload.content, MAX_INLINE_TEXT_CHARS);
            Some(TimelineEvent {
                ts_unix_ms: seq as i64,
                seq,
                kind: TimelineKind::AssistantOutput,
                action_id: None,
                line: format!("assistant_output content={content}"),
            })
        }
        PromptEvent::ExecutionRequested(payload) => {
            let args_preview = preview_to_inline(&payload.args_preview);
            let background = if payload.background {
                " background=true"
            } else {
                ""
            };
            Some(TimelineEvent {
                ts_unix_ms: seq as i64,
                seq,
                kind: TimelineKind::ExecutionRequested,
                action_id: Some(payload.action_id.clone()),
                line: format!(
                    "execution_requested execution_id={} action_id={}{} args_preview={}",
                    payload.execution_id, payload.action_id, background, args_preview
                ),
            })
        }
        PromptEvent::ExecutionSucceeded(payload) => {
            let payload_preview = preview_to_inline(&payload.payload_preview);
            Some(TimelineEvent {
                ts_unix_ms: seq as i64,
                seq,
                kind: TimelineKind::ExecutionSucceeded,
                action_id: Some(payload.action_id.clone()),
                line: format!(
                    "execution_succeeded execution_id={} action_id={} payload_preview={}",
                    payload.execution_id, payload.action_id, payload_preview
                ),
            })
        }
        PromptEvent::ExecutionFailed(payload) => Some(TimelineEvent {
            ts_unix_ms: seq as i64,
            seq,
            kind: TimelineKind::ExecutionFailed,
            action_id: Some(payload.action_id.clone()),
            line: format!(
                "execution_failed execution_id={} action_id={} message={}{}",
                payload.execution_id,
                payload.action_id,
                truncate_inline(&payload.message, MAX_INLINE_TEXT_CHARS),
                payload
                    .payload_preview
                    .as_ref()
                    .map(|preview| format!(" payload_preview={}", preview_to_inline(preview)))
                    .unwrap_or_default()
            ),
        }),
        PromptEvent::ExecutionBackgrounded(payload) => Some(TimelineEvent {
            ts_unix_ms: seq as i64,
            seq,
            kind: TimelineKind::ExecutionBackgrounded,
            action_id: Some(payload.action_id.clone()),
            line: format!(
                "execution_backgrounded execution_id={} action_id={}",
                payload.execution_id, payload.action_id
            ),
        }),
        PromptEvent::ExecutionCanceled(payload) => Some(TimelineEvent {
            ts_unix_ms: seq as i64,
            seq,
            kind: TimelineKind::ExecutionCanceled,
            action_id: Some(payload.action_id.clone()),
            line: format!(
                "execution_canceled execution_id={} action_id={}",
                payload.execution_id, payload.action_id
            ),
        }),
        PromptEvent::ExecutionRejected(payload) => Some(TimelineEvent {
            ts_unix_ms: seq as i64,
            seq,
            kind: TimelineKind::ExecutionRejected,
            action_id: Some(payload.action_id.clone()),
            line: format!(
                "execution_rejected execution_id={} action_id={} message={}",
                payload.execution_id,
                payload.action_id,
                truncate_inline(&payload.message, MAX_INLINE_TEXT_CHARS)
            ),
        }),
        PromptEvent::PayloadLookupAvailable(_)
        | PromptEvent::RetryFeedback(_)
        | PromptEvent::Heartbeat
        | PromptEvent::Cron(_)
        | PromptEvent::RefreshProfile(_) => None,
    }
}
