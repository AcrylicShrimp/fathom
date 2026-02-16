use crate::pb;
use crate::util::{refresh_scope_label, task_status_label};

#[derive(Debug, Clone)]
pub(crate) enum EventRecord {
    Local {
        message: String,
    },
    Session {
        session_id: String,
        kind: SessionEventRecordKind,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum SessionEventRecordKind {
    TriggerAccepted {
        queue_depth: u64,
        trigger_id: String,
    },
    TurnStarted {
        turn_id: u64,
        trigger_count: u64,
    },
    TurnEnded {
        turn_id: u64,
        reason: String,
        history_size: u64,
    },
    AssistantOutput {
        content: String,
        stream_id: String,
    },
    AssistantStream {
        stream_id: String,
        delta: String,
        done: bool,
        user_id: String,
    },
    TaskStateChanged {
        task_id: String,
        status: String,
    },
    ProfileRefreshed {
        scope: String,
        refreshed_user_ids: Vec<String>,
    },
    AgentStream {
        phase: String,
        detail: String,
    },
    TurnFailure {
        turn_id: u64,
        reason_code: String,
        message: String,
    },
    Unknown,
}

impl EventRecord {
    pub(crate) fn local(message: impl Into<String>) -> Self {
        Self::Local {
            message: message.into(),
        }
    }
}

pub(crate) fn session_event_to_record(event: &pb::SessionEvent) -> EventRecord {
    let Some(kind) = event.kind.as_ref() else {
        return EventRecord::Session {
            session_id: event.session_id.clone(),
            kind: SessionEventRecordKind::Unknown,
        };
    };

    let kind = match kind {
        pb::session_event::Kind::TriggerAccepted(data) => SessionEventRecordKind::TriggerAccepted {
            queue_depth: data.queue_depth,
            trigger_id: data
                .trigger
                .as_ref()
                .map(|trigger| trigger.trigger_id.clone())
                .unwrap_or_else(|| "?".to_string()),
        },
        pb::session_event::Kind::TurnStarted(data) => SessionEventRecordKind::TurnStarted {
            turn_id: data.turn_id,
            trigger_count: data.trigger_count,
        },
        pb::session_event::Kind::TurnEnded(data) => SessionEventRecordKind::TurnEnded {
            turn_id: data.turn_id,
            reason: data.reason.clone(),
            history_size: data.history_size,
        },
        pb::session_event::Kind::AssistantOutput(data) => SessionEventRecordKind::AssistantOutput {
            content: data.content.clone(),
            stream_id: data.stream_id.clone(),
        },
        pb::session_event::Kind::AssistantStream(data) => SessionEventRecordKind::AssistantStream {
            stream_id: data.stream_id.clone(),
            delta: data.delta.clone(),
            done: data.done,
            user_id: data.user_id.clone(),
        },
        pb::session_event::Kind::TaskStateChanged(data) => {
            SessionEventRecordKind::TaskStateChanged {
                task_id: data
                    .task
                    .as_ref()
                    .map(|task| task.task_id.clone())
                    .unwrap_or_else(|| "?".to_string()),
                status: data
                    .task
                    .as_ref()
                    .and_then(|task| pb::TaskStatus::try_from(task.status).ok())
                    .map(task_status_label)
                    .unwrap_or("unknown")
                    .to_string(),
            }
        }
        pb::session_event::Kind::ProfileRefreshed(data) => {
            SessionEventRecordKind::ProfileRefreshed {
                scope: refresh_scope_label(
                    pb::RefreshScope::try_from(data.scope).unwrap_or(pb::RefreshScope::Unspecified),
                )
                .to_string(),
                refreshed_user_ids: data.refreshed_user_ids.clone(),
            }
        }
        pb::session_event::Kind::AgentStream(data) => SessionEventRecordKind::AgentStream {
            phase: data.phase.clone(),
            detail: data.detail.clone(),
        },
        pb::session_event::Kind::TurnFailure(data) => SessionEventRecordKind::TurnFailure {
            turn_id: data.turn_id,
            reason_code: data.reason_code.clone(),
            message: data.message.clone(),
        },
    };

    EventRecord::Session {
        session_id: event.session_id.clone(),
        kind,
    }
}

pub(crate) fn render_event_record(record: &EventRecord) -> String {
    match record {
        EventRecord::Local { message } => message.clone(),
        EventRecord::Session { session_id, kind } => {
            let prefix = format!("[{session_id}]");
            match kind {
                SessionEventRecordKind::TriggerAccepted {
                    queue_depth,
                    trigger_id,
                } => {
                    format!("{prefix} trigger accepted depth={queue_depth} id={trigger_id}")
                }
                SessionEventRecordKind::TurnStarted {
                    turn_id,
                    trigger_count,
                } => {
                    format!("{prefix} turn {turn_id} started ({trigger_count} trigger(s))")
                }
                SessionEventRecordKind::TurnEnded {
                    turn_id,
                    reason,
                    history_size,
                } => {
                    format!("{prefix} turn {turn_id} ended: {reason} (history={history_size})")
                }
                SessionEventRecordKind::AssistantOutput { content, stream_id } => {
                    if stream_id.is_empty() {
                        format!("{prefix} assistant: {content}")
                    } else {
                        format!("{prefix} assistant[{stream_id}]: {content}")
                    }
                }
                SessionEventRecordKind::AssistantStream {
                    stream_id,
                    delta,
                    done,
                    user_id,
                } => {
                    let preview = if delta.is_empty() {
                        "".to_string()
                    } else {
                        format!(" delta={}", delta.replace('\n', "\\n"))
                    };
                    let user_suffix = if user_id.is_empty() {
                        "".to_string()
                    } else {
                        format!(" user_id={user_id}")
                    };
                    format!(
                        "{prefix} assistant_stream id={stream_id} done={done}{user_suffix}{preview}"
                    )
                }
                SessionEventRecordKind::TaskStateChanged { task_id, status } => {
                    format!("{prefix} task {task_id} -> {status}")
                }
                SessionEventRecordKind::ProfileRefreshed {
                    scope,
                    refreshed_user_ids,
                } => {
                    format!(
                        "{prefix} profile refreshed scope={scope} users={}",
                        refreshed_user_ids.join(",")
                    )
                }
                SessionEventRecordKind::AgentStream { phase, detail } => {
                    format!("{prefix} agent stream [{phase}] {detail}")
                }
                SessionEventRecordKind::TurnFailure {
                    turn_id,
                    reason_code,
                    message,
                } => {
                    format!("{prefix} turn {turn_id} failed [{reason_code}]: {message}")
                }
                SessionEventRecordKind::Unknown => format!("{prefix} event without payload"),
            }
        }
    }
}
