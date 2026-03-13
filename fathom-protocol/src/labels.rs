use crate::pb;

pub fn execution_status_label(status: pb::ExecutionStatus) -> &'static str {
    match status {
        pb::ExecutionStatus::Unspecified => "unspecified",
        pb::ExecutionStatus::Pending => "pending",
        pb::ExecutionStatus::Running => "running",
        pb::ExecutionStatus::Succeeded => "succeeded",
        pb::ExecutionStatus::Failed => "failed",
        pb::ExecutionStatus::Canceled => "canceled",
    }
}

pub fn refresh_scope_label(scope: pb::RefreshScope) -> &'static str {
    match scope {
        pb::RefreshScope::Unspecified => "unspecified",
        pb::RefreshScope::Agent => "agent",
        pb::RefreshScope::User => "user",
        pb::RefreshScope::All => "all",
    }
}

pub fn system_notice_level_label(level: pb::SystemNoticeLevel) -> &'static str {
    match level {
        pb::SystemNoticeLevel::Unspecified => "unspecified",
        pb::SystemNoticeLevel::Info => "info",
        pb::SystemNoticeLevel::Warning => "warning",
        pb::SystemNoticeLevel::Error => "error",
    }
}

pub fn execution_update_phase_label(phase: pb::ExecutionUpdatePhase) -> &'static str {
    match phase {
        pb::ExecutionUpdatePhase::Unspecified => "unspecified",
        pb::ExecutionUpdatePhase::ArgumentsDelta => "arguments.delta",
        pb::ExecutionUpdatePhase::ArgumentsReady => "arguments.ready",
        pb::ExecutionUpdatePhase::ExecutionSucceeded => "execution_succeeded",
        pb::ExecutionUpdatePhase::ExecutionFailed => "execution_failed",
        pb::ExecutionUpdatePhase::ExecutionBackgrounded => "execution_backgrounded",
        pb::ExecutionUpdatePhase::ExecutionRejected => "execution_rejected",
        pb::ExecutionUpdatePhase::ExecutionCanceled => "execution_canceled",
    }
}
