use crate::pb;

pub(crate) fn execution_status_label(status: pb::ExecutionStatus) -> &'static str {
    match status {
        pb::ExecutionStatus::Unspecified => "unspecified",
        pb::ExecutionStatus::Pending => "pending",
        pb::ExecutionStatus::Running => "running",
        pb::ExecutionStatus::Succeeded => "succeeded",
        pb::ExecutionStatus::Failed => "failed",
        pb::ExecutionStatus::Canceled => "canceled",
    }
}

pub(crate) fn refresh_scope_label(scope: pb::RefreshScope) -> &'static str {
    match scope {
        pb::RefreshScope::Unspecified => "unspecified",
        pb::RefreshScope::Agent => "agent",
        pb::RefreshScope::User => "user",
        pb::RefreshScope::All => "all",
    }
}

pub(crate) fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_millis() as i64
}
