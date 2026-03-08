use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::agent::SummaryBlockRefSnapshot;
use crate::history::HistoryEvent;
use crate::history::schema::HistoryEventKind;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;

const MIN_LIVE_HISTORY_EVENTS: usize = 48;
const COMPACTION_BATCH_EVENTS: usize = 24;
const MAX_SUMMARY_ACTIONS: usize = 4;
const MAX_SUMMARY_USERS: usize = 4;

pub(crate) fn maybe_compact_history(state: &mut SessionState) {
    while state.history.len() > MIN_LIVE_HISTORY_EVENTS + COMPACTION_BATCH_EVENTS {
        let compactable = state.history.len().saturating_sub(MIN_LIVE_HISTORY_EVENTS);
        let batch_len =
            adjusted_batch_len(&state.history, compactable.min(COMPACTION_BATCH_EVENTS));
        if batch_len == 0 {
            break;
        }

        let source_range_start = state.compaction.last_compacted_history_index;
        let source_range_end = source_range_start + batch_len as u64;
        let batch = state.history.drain(0..batch_len).collect::<Vec<_>>();
        let block_id = format!("history-summary-{source_range_end:06}");
        let summary_text =
            summarize_history_batch(&block_id, &batch, source_range_start, source_range_end);

        state
            .compaction
            .summary_blocks
            .push(SummaryBlockRefSnapshot {
                id: block_id,
                source_range_start,
                source_range_end,
                summary_text,
                created_at_unix_ms: now_unix_ms(),
            });
        state.compaction.last_compacted_history_index = source_range_end;
    }
}

fn adjusted_batch_len(history: &[HistoryEvent], proposed: usize) -> usize {
    let mut batch_len = proposed.min(history.len());
    while batch_len > 0 && batch_len < history.len() {
        let last_compacted = &history[batch_len - 1];
        let next_live = &history[batch_len];

        let splits_task_done_finish_pair = matches!(
            (&last_compacted.kind, &next_live.kind),
            (
                HistoryEventKind::TriggerTaskDone(_),
                HistoryEventKind::TaskFinished(_)
            )
        ) && last_compacted.actor_id == next_live.actor_id;
        let splits_task_start_finish_pair = matches!(
            (&last_compacted.kind, &next_live.kind),
            (
                HistoryEventKind::TaskStarted(_),
                HistoryEventKind::TaskFinished(_)
            )
        ) && last_compacted.actor_id == next_live.actor_id;

        if !splits_task_done_finish_pair && !splits_task_start_finish_pair {
            break;
        }
        batch_len -= 1;
    }
    batch_len
}

fn summarize_history_batch(
    block_id: &str,
    batch: &[HistoryEvent],
    source_range_start: u64,
    source_range_end: u64,
) -> String {
    if batch.is_empty() {
        return format!("{block_id} source=[{source_range_start},{source_range_end}) events=0");
    }

    let mut counts = HashMap::<&'static str, usize>::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut actions = BTreeSet::<String>::new();
    let mut users = BTreeSet::<String>::new();

    for event in batch {
        *counts.entry(event.kind.summary_group()).or_default() += 1;

        if matches!(&event.kind, HistoryEventKind::TriggerUserMessage(_)) {
            users.insert(event.actor_id.clone());
        }
        if let Some(status) = event.kind.status() {
            *statuses.entry(status.to_string()).or_default() += 1;
        }
        if let Some(action_id) = event.kind.canonical_action_id() {
            actions.insert(action_id.to_string());
        }
    }

    let first_ts = batch
        .first()
        .map(|event| event.ts_unix_ms)
        .unwrap_or_default();
    let last_ts = batch
        .last()
        .map(|event| event.ts_unix_ms)
        .unwrap_or_default();
    let statuses_preview = statuses
        .into_iter()
        .map(|(status, count)| format!("{status}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    let actions_preview = actions
        .into_iter()
        .take(MAX_SUMMARY_ACTIONS)
        .collect::<Vec<_>>()
        .join(",");
    let users_preview = users
        .into_iter()
        .take(MAX_SUMMARY_USERS)
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "{block_id} source=[{source_range_start},{source_range_end}) ts=[{first_ts},{last_ts}] events={} user_message={} assistant_output={} task_started={} task_finished={} task_done={} refresh_profile={} heartbeat={} cron={} statuses=[{}] actions=[{}] users=[{}]",
        batch.len(),
        counts.get("user_message").copied().unwrap_or_default(),
        counts.get("assistant_output").copied().unwrap_or_default(),
        counts.get("task_started").copied().unwrap_or_default(),
        counts.get("task_finished").copied().unwrap_or_default(),
        counts.get("task_done").copied().unwrap_or_default(),
        counts.get("refresh_profile").copied().unwrap_or_default(),
        counts.get("heartbeat").copied().unwrap_or_default(),
        counts.get("cron").copied().unwrap_or_default(),
        statuses_preview,
        actions_preview,
        users_preview
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::{COMPACTION_BATCH_EVENTS, MIN_LIVE_HISTORY_EVENTS, maybe_compact_history};
    use crate::environment::EnvironmentRegistry;
    use crate::history::schema::{
        HistoryActorKind, HistoryEventKind, TaskFinishedHistoryPayload, UserMessageHistoryPayload,
    };
    use crate::history::{HistoryEvent, PayloadPreview};
    use crate::session::SessionState;
    use crate::util::{default_agent_profile, default_user_profile};

    fn test_state() -> SessionState {
        let user_id = "user-a".to_string();
        SessionState::new(
            "session-1".to_string(),
            "agent-a".to_string(),
            vec![user_id.clone()],
            default_agent_profile("agent-a"),
            HashMap::from([(user_id.clone(), default_user_profile(&user_id))]),
            EnvironmentRegistry::default_engaged_environment_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            EnvironmentRegistry::initial_environment_snapshots()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        )
    }

    #[test]
    fn compacts_old_history_into_summary_blocks() {
        let mut state = test_state();
        for index in 0..80 {
            state.history.push(HistoryEvent {
                ts_unix_ms: index,
                actor_kind: if index % 2 == 0 {
                    HistoryActorKind::User
                } else {
                    HistoryActorKind::Task
                },
                actor_id: if index % 2 == 0 {
                    "user-a".to_string()
                } else {
                    format!("task-{index}")
                },
                profile_ref: "test".to_string(),
                kind: if index % 2 == 0 {
                    HistoryEventKind::TriggerUserMessage(UserMessageHistoryPayload {
                        text: format!("message-{index}"),
                    })
                } else {
                    HistoryEventKind::TaskFinished(TaskFinishedHistoryPayload {
                        canonical_action_id: "filesystem__list".to_string(),
                        environment_id: "filesystem".to_string(),
                        action_name: "list".to_string(),
                        status: "succeeded".to_string(),
                        result_preview: PayloadPreview {
                            head: "[]".to_string(),
                            tail: String::new(),
                            full_bytes: 2,
                            head_bytes: 2,
                            tail_bytes: 0,
                            truncated: false,
                            omitted_bytes: 0,
                            lookup_ref: format!("task://task-{index}/result"),
                        },
                        lookup_action: "system__get_task_payload".to_string(),
                    })
                },
            });
        }

        maybe_compact_history(&mut state);

        assert!(!state.compaction.summary_blocks.is_empty());
        assert!(state.compaction.last_compacted_history_index > 0);
        assert!(state.history.len() <= MIN_LIVE_HISTORY_EVENTS + COMPACTION_BATCH_EVENTS);
    }
}
