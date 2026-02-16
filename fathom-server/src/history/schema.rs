use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistoryActorKind {
    User,
    Assistant,
    System,
    Task,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryEventLine {
    pub(crate) ts_unix_ms: i64,
    pub(crate) event: String,
    pub(crate) actor_kind: HistoryActorKind,
    pub(crate) actor_id: String,
    pub(crate) profile_ref: String,
    pub(crate) payload: Value,
}

impl HistoryEventLine {
    pub(crate) fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                "{{\"event\":\"{}\",\"actor_id\":\"{}\"}}",
                self.event, self.actor_id
            )
        })
    }
}
