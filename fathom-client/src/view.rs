use crate::pb;
use crate::util::{refresh_scope_label, task_status_label};

pub(crate) fn render_event(event: &pb::SessionEvent) -> String {
    let prefix = format!("[{}]", event.session_id);
    let Some(kind) = event.kind.as_ref() else {
        return format!("{prefix} event without payload");
    };

    match kind {
        pb::session_event::Kind::TriggerAccepted(data) => format!(
            "{prefix} trigger accepted depth={} id={}",
            data.queue_depth,
            data.trigger
                .as_ref()
                .map(|trigger| trigger.trigger_id.as_str())
                .unwrap_or("?")
        ),
        pb::session_event::Kind::TurnStarted(data) => {
            format!(
                "{prefix} turn {} started ({} trigger(s))",
                data.turn_id, data.trigger_count
            )
        }
        pb::session_event::Kind::TurnEnded(data) => format!(
            "{prefix} turn {} ended: {} (history={})",
            data.turn_id, data.reason, data.history_size
        ),
        pb::session_event::Kind::AssistantOutput(data) => {
            format!("{prefix} assistant: {}", data.content)
        }
        pb::session_event::Kind::TaskStateChanged(data) => {
            let task = data.task.as_ref();
            format!(
                "{prefix} task {} -> {}",
                task.map(|task| task.task_id.as_str()).unwrap_or("?"),
                task.and_then(|task| pb::TaskStatus::try_from(task.status).ok())
                    .map(task_status_label)
                    .unwrap_or("unknown")
            )
        }
        pb::session_event::Kind::ProfileRefreshed(data) => format!(
            "{prefix} profile refreshed scope={} users={}",
            refresh_scope_label(
                pb::RefreshScope::try_from(data.scope).unwrap_or(pb::RefreshScope::Unspecified)
            ),
            data.refreshed_user_ids.join(",")
        ),
    }
}
