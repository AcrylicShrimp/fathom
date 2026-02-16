use crate::history;
use crate::pb;
use crate::session::state::SessionState;

pub(super) fn flush_history(
    state: &mut SessionState,
    turn_triggers: &[pb::Trigger],
    assistant_outputs: &[String],
) {
    for trigger in turn_triggers {
        history::append_trigger_history(state, trigger);
    }

    for output in assistant_outputs {
        history::append_assistant_output_history(state, output);
    }
}
