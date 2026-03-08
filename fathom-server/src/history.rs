mod compaction;
mod constants;
mod preview;
pub(crate) mod schema;
mod transform;

use crate::pb;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;

use self::compaction::maybe_compact_history;

pub(crate) use constants::TASK_PAYLOAD_LOOKUP_ACTION;
pub(crate) use preview::{PayloadPreview, build_payload_preview};
pub(crate) use schema::{HistoryEvent, HistoryEventKind};

pub(crate) fn append_trigger_history(state: &mut SessionState, trigger: &pb::Trigger) {
    state.history.push(transform::trigger_line(state, trigger));
    maybe_compact_history(state);
}

pub(crate) fn append_assistant_output_history(state: &mut SessionState, content: &str) {
    state.history.push(transform::assistant_output_line(
        state,
        now_unix_ms(),
        content,
    ));
    maybe_compact_history(state);
}

pub(crate) fn append_task_started_history(state: &mut SessionState, task: &pb::Task) {
    state
        .history
        .push(transform::task_started_line(state, task));
    maybe_compact_history(state);
}

pub(crate) fn append_task_finished_history(state: &mut SessionState, task: &pb::Task) {
    state
        .history
        .push(transform::task_finished_line(state, task));
    maybe_compact_history(state);
}
