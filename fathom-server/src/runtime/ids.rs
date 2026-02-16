use std::sync::atomic::Ordering;

use super::Runtime;

impl Runtime {
    pub(super) fn next_session_id(&self) -> String {
        format!(
            "session-{}",
            self.inner.session_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub(crate) fn next_trigger_id(&self) -> String {
        format!(
            "trigger-{}",
            self.inner.trigger_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub(crate) fn next_task_id(&self) -> String {
        format!(
            "task-{}",
            self.inner.task_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }
}
