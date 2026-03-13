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

    pub(crate) fn next_execution_id(&self) -> String {
        format!(
            "execution-{}",
            self.inner.execution_seq.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    pub(crate) fn next_execution_submission_id(&self) -> String {
        format!(
            "execution-submission-{}",
            self.inner
                .execution_submission_seq
                .fetch_add(1, Ordering::Relaxed)
                + 1
        )
    }
}
