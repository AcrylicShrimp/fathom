mod compaction;
mod constants;
mod preview;
pub(crate) mod schema;
mod transform;

use crate::session::state::SessionState;
use crate::util::now_unix_ms;
use fathom_protocol::pb;

use self::compaction::maybe_compact_history;

pub(crate) use constants::{EXECUTION_INPUT_LOOKUP_ACTION, EXECUTION_RESULT_LOOKUP_ACTION};
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

pub(crate) fn append_execution_requested_history(
    state: &mut SessionState,
    execution: &pb::Execution,
) {
    state
        .history
        .push(transform::execution_requested_line(state, execution));
    maybe_compact_history(state);
}
