use std::hash::{Hash, Hasher};

use crate::agent::types::{CompiledPrompt, PromptMessage, PromptMessageDiagnostic};

use super::timeline::TimelineBuild;

pub(super) fn push_message<F>(
    bundle: &mut CompiledPrompt,
    role: &str,
    label: &str,
    content: String,
    token_estimator: F,
) where
    F: Fn(&str) -> usize,
{
    let message = PromptMessage::new(role.to_string(), label.to_string(), content);
    let stat = PromptMessageDiagnostic {
        label: label.to_string(),
        role: role.to_string(),
        estimated_tokens: token_estimator(&message.content),
        stable_hash: message.stable_hash.clone(),
    };
    bundle.messages.push(message);
    bundle.diagnostics.per_message.push(stat);
}

pub(super) fn finalize_compiled_prompt(
    bundle: &mut CompiledPrompt,
    timeline: &TimelineBuild,
    compacted_events: usize,
    compaction_applied: bool,
    compaction_reason: String,
) {
    bundle.diagnostics.timeline_raw_events = timeline.raw_events;
    bundle.diagnostics.timeline_compacted_events = compacted_events;
    bundle.diagnostics.dedup_dropped_events = timeline.dedup_dropped;
    bundle.diagnostics.compaction_applied = compaction_applied;
    bundle.diagnostics.compaction_reason = compaction_reason;
    bundle.diagnostics.messages_count = bundle.messages.len();
    bundle.diagnostics.estimated_prompt_tokens = bundle
        .diagnostics
        .per_message
        .iter()
        .map(|item| item.estimated_tokens)
        .sum::<usize>();
    bundle.diagnostics.stable_prefix_hash = stable_prefix_hash(&bundle.messages);
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
