mod chunking;
mod compaction;
mod diagnostics;
mod render;
#[cfg(test)]
mod tests;
mod timeline;
mod util;

use crate::agent::types::{CompiledPrompt, PromptInput};

use self::chunking::chunk_section_messages;
use self::compaction::{build_session_compaction_summaries, compact_timeline};
use self::diagnostics::{finalize_compiled_prompt, push_message};
use self::render::{
    build_harness_contract_block, build_identity_envelope_block, build_session_baseline_block,
    build_tail_event_lines, render_event_transcript_lines,
};
use self::timeline::build_canonical_timeline;
use self::util::estimate_tokens;

pub(super) const TOKEN_DIVISOR_CHARS: usize = 4;
pub(super) const DEFAULT_CONTEXT_LIMIT_TOKENS: usize = 128_000;
pub(super) const DEFAULT_SOFT_CONTEXT_RATIO: f64 = 0.70;
pub(super) const DEFAULT_HARD_CONTEXT_RATIO: f64 = 0.85;
pub(super) const TIMELINE_SECTION_MAX_TOKENS: usize = 2_500;
pub(super) const MIN_TIMELINE_EVENTS_AFTER_COMPACTION: usize = 18;
pub(super) const MIN_TIMELINE_EVENTS_AFTER_HARD_TRIM: usize = 8;
pub(super) const COMPACTION_BATCH_EVENTS: usize = 12;
pub(super) const MAX_SESSION_SUMMARY_BLOCKS_IN_PROMPT: usize = 8;
pub(super) const MAX_INLINE_TEXT_CHARS: usize = 320;
pub(super) const MAX_PREVIEW_HEAD_CHARS: usize = 180;
pub(super) const MAX_PREVIEW_TAIL_CHARS: usize = 120;
pub(super) const MAX_LOOKUP_PAYLOAD_CHARS: usize = 1_600;

#[derive(Debug, Clone, Default)]
pub(crate) struct PromptCompiler;

impl PromptCompiler {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn compile(&self, input: &PromptInput) -> CompiledPrompt {
        let harness_contract = build_harness_contract_block(input);
        let identity_envelope = build_identity_envelope_block(input);
        let session_baseline = build_session_baseline_block(input);
        let tail_event_lines = build_tail_event_lines(input);

        let timeline = build_canonical_timeline(input);
        let (session_summary_lines, session_summary_count) =
            build_session_compaction_summaries(input);

        let non_timeline_estimated = estimate_tokens(&harness_contract)
            + estimate_tokens(&identity_envelope)
            + estimate_tokens(&session_baseline)
            + estimate_tokens(&tail_event_lines.join("\n"));
        let (timeline_events, summary_lines, compaction_reason, compacted_events) =
            compact_timeline(
                &timeline.events,
                &session_summary_lines,
                session_summary_count,
                non_timeline_estimated,
            );

        let event_lines = render_event_transcript_lines(&summary_lines, &timeline_events);
        let event_messages = chunk_section_messages(
            "event_transcript",
            "## Event Transcript",
            &event_lines,
            TIMELINE_SECTION_MAX_TOKENS,
        );
        let tail_messages = if tail_event_lines.is_empty() {
            Vec::new()
        } else {
            chunk_section_messages(
                "pending_inputs",
                "## Pending Inputs",
                &tail_event_lines,
                TIMELINE_SECTION_MAX_TOKENS,
            )
        };

        let mut bundle = CompiledPrompt::default();
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
        for (label, content) in tail_messages {
            push_message(&mut bundle, "user", &label, content, estimate_tokens);
        }

        finalize_compiled_prompt(
            &mut bundle,
            &timeline,
            compacted_events,
            !summary_lines.is_empty() || compacted_events > 0,
            compaction_reason,
        );
        bundle
    }
}
