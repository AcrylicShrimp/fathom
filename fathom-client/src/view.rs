use fathom_protocol::pb;
use fathom_protocol::{
    execution_status_label, execution_update_phase_label, refresh_scope_label,
    system_notice_level_label,
};

const EXECUTION_ARGS_PREVIEW_MAX_CHARS: usize = 140;
const EXECUTION_RESULT_PREVIEW_MAX_CHARS: usize = 160;
const EXECUTION_UPDATE_ARGS_PREVIEW_MAX_CHARS: usize = 120;

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
    ExecutionStateChanged {
        execution_id: String,
        action_id: String,
        status: String,
        args_json: String,
        args_preview: String,
        result_message: String,
        result_preview: String,
    },
    ProfileRefreshed {
        scope: String,
        refreshed_user_ids: Vec<String>,
    },
    SystemNotice {
        level: String,
        code: String,
        message: String,
    },
    ExecutionUpdate {
        phase: String,
        call_key: String,
        call_id: String,
        action_id: String,
        execution_id: String,
        args_preview: String,
        detail: String,
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
        pb::session_event::Kind::ExecutionStateChanged(data) => {
            let execution = data.execution.as_ref();
            let execution_id = execution
                .map(|execution| execution.execution_id.clone())
                .unwrap_or_else(|| "?".to_string());
            let action_id = execution
                .map(|execution| execution.action_id.trim().to_string())
                .filter(|value: &String| !value.is_empty())
                .unwrap_or_else(|| "?".to_string());
            let args_json = execution
                .map(|execution| execution.args_json.clone())
                .unwrap_or_else(|| "{}".to_string());
            let result_message = execution
                .map(|execution| execution.result_message.clone())
                .unwrap_or_default();
            SessionEventRecordKind::ExecutionStateChanged {
                execution_id,
                action_id,
                status: execution
                    .and_then(|execution| pb::ExecutionStatus::try_from(execution.status).ok())
                    .map(execution_status_label)
                    .unwrap_or("unknown")
                    .to_string(),
                args_preview: summarize_for_preview(&args_json, EXECUTION_ARGS_PREVIEW_MAX_CHARS),
                args_json,
                result_preview: summarize_for_preview(
                    &result_message,
                    EXECUTION_RESULT_PREVIEW_MAX_CHARS,
                ),
                result_message,
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
        pb::session_event::Kind::SystemNotice(data) => SessionEventRecordKind::SystemNotice {
            level: system_notice_level_label(
                pb::SystemNoticeLevel::try_from(data.level)
                    .unwrap_or(pb::SystemNoticeLevel::Unspecified),
            )
            .to_string(),
            code: data.code.clone(),
            message: data.message.clone(),
        },
        pb::session_event::Kind::ExecutionUpdate(data) => {
            let args_source = if data.args_json.trim().is_empty() {
                data.args_delta.as_str()
            } else {
                data.args_json.as_str()
            };
            SessionEventRecordKind::ExecutionUpdate {
                phase: execution_update_phase_label(
                    pb::ExecutionUpdatePhase::try_from(data.phase)
                        .unwrap_or(pb::ExecutionUpdatePhase::Unspecified),
                )
                .to_string(),
                call_key: data.call_key.clone(),
                call_id: data.call_id.clone(),
                action_id: data.action_id.clone(),
                execution_id: data.execution_id.clone(),
                args_preview: if args_source.trim().is_empty() {
                    String::new()
                } else {
                    summarize_for_preview(args_source, EXECUTION_UPDATE_ARGS_PREVIEW_MAX_CHARS)
                },
                detail: data.detail.clone(),
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
                SessionEventRecordKind::ExecutionStateChanged {
                    execution_id,
                    action_id,
                    status,
                    args_preview,
                    result_preview,
                    ..
                } => {
                    let mut line = format!(
                        "{prefix} execution {execution_id} {action_id} -> {status} args={args_preview}"
                    );
                    if (status == "failed" || status == "canceled") && !result_preview.is_empty() {
                        line.push_str(&format!(" result={result_preview}"));
                    }
                    line
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
                SessionEventRecordKind::SystemNotice {
                    level,
                    code,
                    message,
                } => {
                    if code.is_empty() {
                        format!("{prefix} system notice [{level}] {message}")
                    } else {
                        format!("{prefix} system notice [{level}] {code}: {message}")
                    }
                }
                SessionEventRecordKind::ExecutionUpdate {
                    phase,
                    call_key,
                    call_id,
                    action_id,
                    execution_id,
                    args_preview,
                    detail,
                } => {
                    let mut line = format!("{prefix} execution_update {phase}");
                    if !action_id.is_empty() {
                        line.push_str(&format!(" action={action_id}"));
                    }
                    if !execution_id.is_empty() {
                        line.push_str(&format!(" execution={execution_id}"));
                    }
                    if !call_id.is_empty() {
                        line.push_str(&format!(" call_id={call_id}"));
                    } else if !call_key.is_empty() {
                        line.push_str(&format!(" call={call_key}"));
                    }
                    if !args_preview.is_empty()
                        && phase != "awaited_execution_succeeded"
                        && phase != "awaited_execution_failed"
                        && phase != "execution_detached"
                        && phase != "detached_execution_succeeded"
                        && phase != "detached_execution_failed"
                        && phase != "execution_rejected"
                    {
                        line.push_str(&format!(" args={args_preview}"));
                    }
                    if !detail.is_empty() {
                        line.push_str(&format!(" detail={detail}"));
                    }
                    line
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

fn summarize_for_preview(source: &str, max_chars: usize) -> String {
    let normalized = normalize_json_if_possible(source);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }

    let escaped = trimmed.replace('\n', "\\n");
    let char_count = escaped.chars().count();
    if char_count <= max_chars {
        return escaped;
    }

    let mut prefix = String::new();
    for ch in escaped.chars().take(max_chars) {
        prefix.push(ch);
    }
    let omitted = char_count.saturating_sub(max_chars);
    format!("{prefix}... ({omitted} chars omitted)")
}

fn normalize_json_if_possible(source: &str) -> String {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    match serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|value| serde_json::to_string(&value).ok())
    {
        Some(normalized) => normalized,
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{render_event_record, session_event_to_record};
    use fathom_protocol::pb;

    #[test]
    fn execution_event_render_includes_action_and_args_preview() {
        let event = pb::SessionEvent {
            session_id: "s1".to_string(),
            created_at_unix_ms: 0,
            kind: Some(pb::session_event::Kind::ExecutionStateChanged(
                pb::ExecutionStateChangedEvent {
                    execution: Some(pb::Execution {
                        execution_id: "execution-1".to_string(),
                        session_id: "s1".to_string(),
                        action_id: "filesystem__list".to_string(),
                        args_json: r#"{"path":"."}"#.to_string(),
                        status: pb::ExecutionStatus::Running as i32,
                        result_message: String::new(),
                        created_at_unix_ms: 0,
                        updated_at_unix_ms: 0,
                    }),
                },
            )),
        };
        let record = session_event_to_record(&event);
        let line = render_event_record(&record);

        assert!(line.contains("execution-1 filesystem__list -> running"));
        assert!(line.contains(r#"args={"path":"."}"#));
    }

    #[test]
    fn execution_event_render_includes_failed_result_preview() {
        let event = pb::SessionEvent {
            session_id: "s1".to_string(),
            created_at_unix_ms: 0,
            kind: Some(pb::session_event::Kind::ExecutionStateChanged(
                pb::ExecutionStateChangedEvent {
                    execution: Some(pb::Execution {
                        execution_id: "execution-2".to_string(),
                        session_id: "s1".to_string(),
                        action_id: "filesystem__read".to_string(),
                        args_json: r#"{"path":"notes.txt"}"#.to_string(),
                        status: pb::ExecutionStatus::Failed as i32,
                        result_message: "not found\nthis file does not exist in the workspace"
                            .to_string(),
                        created_at_unix_ms: 0,
                        updated_at_unix_ms: 0,
                    }),
                },
            )),
        };
        let record = session_event_to_record(&event);
        let line = render_event_record(&record);

        assert!(line.contains("-> failed"));
        assert!(line.contains("result=not found\\nthis file does not exist"));
    }

    #[test]
    fn execution_update_render_includes_phase_and_execution() {
        let event = pb::SessionEvent {
            session_id: "s1".to_string(),
            created_at_unix_ms: 0,
            kind: Some(pb::session_event::Kind::ExecutionUpdate(
                pb::ExecutionUpdateEvent {
                    phase: pb::ExecutionUpdatePhase::ExecutionDetached as i32,
                    call_key: "call-1".to_string(),
                    call_id: "fc_1".to_string(),
                    action_id: "shell__run".to_string(),
                    execution_id: "execution-1".to_string(),
                    args_delta: String::new(),
                    args_json: r#"{"command":"pwd","execution_mode":"detach"}"#.to_string(),
                    detail: "submitted action `shell__run` as execution-1 (running) mode=detach"
                        .to_string(),
                },
            )),
        };
        let record = session_event_to_record(&event);
        let line = render_event_record(&record);

        assert!(line.contains("execution_update execution_detached"));
        assert!(line.contains("execution=execution-1"));
        assert!(line.contains("call_id=fc_1"));
    }

    #[test]
    fn system_notice_event_render_includes_level_and_code() {
        let event = pb::SessionEvent {
            session_id: "s1".to_string(),
            created_at_unix_ms: 0,
            kind: Some(pb::session_event::Kind::SystemNotice(
                pb::SystemNoticeEvent {
                    level: pb::SystemNoticeLevel::Info as i32,
                    code: "profile_refresh".to_string(),
                    message: "profile copies refreshed for this session".to_string(),
                },
            )),
        };
        let record = session_event_to_record(&event);
        let line = render_event_record(&record);

        assert!(line.contains("system notice [info]"));
        assert!(line.contains("profile_refresh"));
    }
}
