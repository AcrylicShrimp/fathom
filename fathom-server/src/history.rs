mod constants;
mod preview;
mod schema;
mod transform;

use crate::pb;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;

pub(crate) use constants::{
    HISTORY_FORMAT, TASK_FINISHED_EVENT, TASK_PAYLOAD_LOOKUP_ACTION, TASK_STARTED_EVENT,
};
pub(crate) use preview::build_payload_preview;

pub(crate) fn append_trigger_history(state: &mut SessionState, trigger: &pb::Trigger) {
    state.history.push(transform::trigger_line(state, trigger));
}

pub(crate) fn append_assistant_output_history(state: &mut SessionState, content: &str) {
    state.history.push(transform::assistant_output_line(
        state,
        now_unix_ms(),
        content,
    ));
}

pub(crate) fn append_task_started_history(state: &mut SessionState, task: &pb::Task) {
    state
        .history
        .push(transform::task_started_line(state, task));
}

pub(crate) fn append_task_finished_history(state: &mut SessionState, task: &pb::Task) {
    state
        .history
        .push(transform::task_finished_line(state, task));
}
