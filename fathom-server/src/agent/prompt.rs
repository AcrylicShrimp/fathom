use crate::agent::types::TurnSnapshot;
use crate::pb;

pub(crate) fn build_agent_prompt(snapshot: &TurnSnapshot, retry_feedback: Option<&str>) -> String {
    let mut lines: Vec<String> = vec![
        "You are Fathom's session agent.".to_string(),
        "You may emit assistant text and/or action calls.".to_string(),
        "When calling actions, use canonical action ids in the format env__action.".to_string(),
        "All actions are server-managed background jobs and emit task_done triggers after commit.".to_string(),
        "Use filesystem__get_base_path/filesystem__list/filesystem__read/filesystem__write/filesystem__replace for base-path-relative file operations.".to_string(),
        "If you need fresher clock data than this snapshot, call system__get_time.".to_string(),
        "If you need environment docs, call system__describe_environment.".to_string(),
        "Task results arrive as JSON text in task_done.result_message.".to_string(),
        String::new(),
    ];

    lines.push("## Session".to_string());
    lines.push(format!("session_id: {}", snapshot.session_id));
    lines.push(format!("turn_id: {}", snapshot.turn_id));
    lines.push(String::new());

    lines.push("## System Context (authoritative)".to_string());
    lines.push(format!(
        "runtime_version: {}",
        snapshot.system_context.runtime_version
    ));
    lines.push("current_time:".to_string());
    lines.push(format!(
        "- utc_rfc3339: {}",
        snapshot.system_context.time_context.utc_rfc3339
    ));
    lines.push(format!(
        "- local_rfc3339: {}",
        snapshot.system_context.time_context.local_rfc3339
    ));
    lines.push(format!(
        "- local_timezone_name: {}",
        snapshot.system_context.time_context.local_timezone_name
    ));
    lines.push(format!(
        "- local_utc_offset: {}",
        snapshot.system_context.time_context.local_utc_offset
    ));
    lines.push(format!(
        "- generated_at_unix_ms: {}",
        snapshot.system_context.time_context.generated_at_unix_ms
    ));
    lines.push(format!(
        "- time_source: {}",
        snapshot.system_context.time_context.time_source
    ));
    lines.push(format!(
        "path_policy.path_format: {}",
        snapshot.system_context.path_policy.path_format
    ));
    lines.push(format!(
        "path_policy.base_path_scope: {}",
        snapshot.system_context.path_policy.base_path_scope
    ));
    lines.push(format!(
        "path_policy.absolute_paths_allowed: {}",
        snapshot.system_context.path_policy.absolute_paths_allowed
    ));
    lines.push(format!(
        "path_policy.escape_outside_base_path_allowed: {}",
        snapshot
            .system_context
            .path_policy
            .escape_outside_base_path_allowed
    ));
    lines.push("activated_environments:".to_string());
    if snapshot.system_context.activated_environments.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for environment in &snapshot.system_context.activated_environments {
            lines.push(format!(
                "- id={} name={} description={}",
                environment.id, environment.name, environment.description
            ));
        }
    }
    lines.push("session_identity:".to_string());
    lines.push(format!(
        "- session_id: {}",
        snapshot.system_context.session_identity.session_id
    ));
    lines.push(format!(
        "- active_agent_id: {}",
        snapshot.system_context.session_identity.active_agent_id
    ));
    lines.push(format!(
        "- active_agent_spec_version: {}",
        snapshot
            .system_context
            .session_identity
            .active_agent_spec_version
    ));
    lines.push(format!(
        "- participant_user_ids: {}",
        snapshot
            .system_context
            .session_identity
            .participant_user_ids
            .join(",")
    ));
    lines.push(format!(
        "- participant_user_updated_at: {:?}",
        snapshot
            .system_context
            .session_identity
            .participant_user_updated_at
    ));
    lines.push(format!(
        "- engaged_environment_ids: {}",
        snapshot
            .system_context
            .session_identity
            .engaged_environment_ids
            .join(",")
    ));
    lines.push("in_flight_actions:".to_string());
    if snapshot
        .system_context
        .session_identity
        .in_flight_actions
        .is_empty()
    {
        lines.push("- (none)".to_string());
    } else {
        for action in &snapshot.system_context.session_identity.in_flight_actions {
            lines.push(format!(
                "- task={} seq={} id={} status={} submitted_at={} args_preview={}",
                action.task_id,
                action.env_seq,
                action.canonical_action_id,
                action.status,
                action.submitted_at_unix_ms,
                action.args_preview
            ));
        }
    }
    lines.push("action_policy:".to_string());
    lines.push(format!(
        "- known_actions: {}",
        snapshot
            .system_context
            .action_policy
            .known_actions
            .join(",")
    ));
    lines.push(format!(
        "- general_actions_trigger_followup_turn: {}",
        snapshot
            .system_context
            .action_policy
            .general_actions_trigger_followup_turn
    ));
    lines.push("history_policy:".to_string());
    lines.push(format!(
        "- format: {}",
        snapshot.system_context.history_policy.format
    ));
    lines.push(format!(
        "- task_started_event: {}",
        snapshot.system_context.history_policy.task_started_event
    ));
    lines.push(format!(
        "- task_finished_event: {}",
        snapshot.system_context.history_policy.task_finished_event
    ));
    lines.push(format!(
        "- preview_max_bytes: {}",
        snapshot.system_context.history_policy.preview_max_bytes
    ));
    lines.push(format!(
        "- preview_max_lines: {}",
        snapshot.system_context.history_policy.preview_max_lines
    ));
    lines.push(format!(
        "- lookup_action: {}",
        snapshot.system_context.history_policy.lookup_action
    ));
    lines.push(String::new());

    lines.push("## Agent Profile Copy".to_string());
    lines.push(format!(
        "display_name: {}",
        snapshot.agent_profile.display_name
    ));
    lines.push("SOUL.md:".to_string());
    lines.push(snapshot.agent_profile.soul_md.clone());
    lines.push("IDENTITY.md:".to_string());
    lines.push(snapshot.agent_profile.identity_md.clone());
    lines.push("AGENTS.md:".to_string());
    lines.push(snapshot.agent_profile.agents_md.clone());
    lines.push("guidelines:".to_string());
    lines.push(snapshot.agent_profile.guidelines_md.clone());
    lines.push(String::new());

    lines.push("## Participant User Profiles".to_string());
    if snapshot.participant_profiles.is_empty() {
        lines.push("(none)".to_string());
    } else {
        for profile in &snapshot.participant_profiles {
            lines.push(format!("- user_id: {}", profile.user_id));
            lines.push(format!("  name: {}", profile.name));
            lines.push(format!("  nickname: {}", profile.nickname));
            lines.push(format!("  preferences_json: {}", profile.preferences_json));
            lines.push("  USER.md:".to_string());
            lines.push(profile.user_md.clone());
        }
    }
    lines.push(String::new());

    lines.push("## Recent History".to_string());
    lines.push("History entries are structured JSON lines.".to_string());
    if snapshot.recent_history.is_empty() {
        lines.push("(empty)".to_string());
    } else {
        for item in &snapshot.recent_history {
            lines.push(item.clone());
        }
    }
    lines.push(String::new());

    lines.push("## Compaction State (modeled, not actively updated yet)".to_string());
    lines.push(format!(
        "last_compacted_history_index: {}",
        snapshot.compaction.last_compacted_history_index
    ));
    if snapshot.compaction.summary_blocks.is_empty() {
        lines.push("summary_blocks: []".to_string());
    } else {
        for block in &snapshot.compaction.summary_blocks {
            lines.push(format!(
                "summary_block: id={} range=[{}, {}] created_at={} text={}",
                block.id,
                block.source_range_start,
                block.source_range_end,
                block.created_at_unix_ms,
                block.summary_text
            ));
        }
    }
    lines.push(String::new());

    lines.push("## Trigger Snapshot For This Turn".to_string());
    for trigger in &snapshot.triggers {
        lines.push(format!("- {}", trigger_text(trigger)));
    }
    lines.push(String::new());

    if let Some(feedback) = retry_feedback {
        lines.push("## Retry Feedback".to_string());
        lines.push(feedback.to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}

fn trigger_text(trigger: &pb::Trigger) -> String {
    let Some(kind) = trigger.kind.as_ref() else {
        return "unknown_trigger".to_string();
    };

    match kind {
        pb::trigger::Kind::UserMessage(message) => {
            format!(
                "user_message user={} text={}",
                message.user_id, message.text
            )
        }
        pb::trigger::Kind::TaskDone(done) => {
            format!(
                "task_done task_id={} result={}",
                done.task_id, done.result_message
            )
        }
        pb::trigger::Kind::Heartbeat(_) => "heartbeat".to_string(),
        pb::trigger::Kind::Cron(cron) => format!("cron key={}", cron.key),
        pb::trigger::Kind::RefreshProfile(refresh) => {
            format!(
                "refresh_profile scope={} user_id={}",
                refresh.scope, refresh.user_id
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::agent::{
        ActivatedEnvironmentHint, SessionCompactionSnapshot, SessionIdentityMapSnapshot,
        SystemContextSnapshot, SystemTimeContext, TurnSnapshot,
    };
    use crate::policy::synthesize_policy_snapshot;
    use crate::util::default_agent_profile;

    use super::build_agent_prompt;

    #[test]
    fn prompt_contains_current_time_block() {
        let policy = synthesize_policy_snapshot(true);
        let snapshot = TurnSnapshot {
            session_id: "session-1".to_string(),
            turn_id: 1,
            system_context: SystemContextSnapshot {
                runtime_version: "0.1.0".to_string(),
                time_context: SystemTimeContext {
                    generated_at_unix_ms: 1_765_000_000_000,
                    utc_rfc3339: "2026-02-16T00:00:00.000Z".to_string(),
                    local_rfc3339: "2026-02-16T09:00:00.000+09:00".to_string(),
                    local_timezone_name: "Asia/Seoul".to_string(),
                    local_utc_offset: "+09:00".to_string(),
                    time_source: "server_clock".to_string(),
                },
                path_policy: policy.path_policy,
                activated_environments: vec![ActivatedEnvironmentHint {
                    id: "filesystem".to_string(),
                    name: "Filesystem".to_string(),
                    description: "Stateful filesystem environment rooted at a base path."
                        .to_string(),
                }],
                session_identity: SessionIdentityMapSnapshot {
                    session_id: "session-1".to_string(),
                    active_agent_id: "agent-default".to_string(),
                    participant_user_ids: vec!["user-default".to_string()],
                    active_agent_spec_version: 1,
                    participant_user_updated_at: BTreeMap::from([(
                        "user-default".to_string(),
                        1_765_000_000_000,
                    )]),
                    engaged_environment_ids: vec!["filesystem".to_string(), "system".to_string()],
                    in_flight_actions: vec![],
                },
                action_policy: policy.action_policy,
                history_policy: policy.history_policy,
            },
            agent_profile: default_agent_profile("agent-default"),
            participant_profiles: vec![],
            triggers: vec![],
            recent_history: vec![],
            compaction: SessionCompactionSnapshot::default(),
        };

        let prompt = build_agent_prompt(&snapshot, None);

        assert!(prompt.contains("current_time:"));
        assert!(prompt.contains("utc_rfc3339: 2026-02-16T00:00:00.000Z"));
        assert!(prompt.contains("local_timezone_name: Asia/Seoul"));
        assert!(prompt.contains("call system__get_time"));
        assert!(prompt.contains("system__describe_environment"));
    }
}
