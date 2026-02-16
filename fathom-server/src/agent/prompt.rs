use crate::agent::ToolRegistry;
use crate::agent::types::TurnSnapshot;
use crate::pb;

pub(crate) fn build_tool_only_prompt(
    snapshot: &TurnSnapshot,
    retry_feedback: Option<&str>,
) -> String {
    let mut lines: Vec<String> = vec![
        "You are Fathom's session agent.".to_string(),
        "You must respond using one or more tool calls only.".to_string(),
        "Never emit plain assistant text as the final answer for this turn.".to_string(),
        "Use send_message to talk to users.".to_string(),
        "send_message is non-triggering: it does not create a task_done follow-up trigger."
            .to_string(),
        "If no action is needed, call schedule_heartbeat with a short delay.".to_string(),
        "All tools are server-managed background jobs.".to_string(),
        "Use fs_list/fs_read/fs_write/fs_replace for file operations.".to_string(),
    ];
    let discovery_tools = ToolRegistry::discovery_tool_names();
    if !discovery_tools.is_empty() {
        lines.push(format!(
            "Use {} for system discovery.",
            discovery_tools.join("/")
        ));
    }
    lines.push("Task results arrive as JSON text in task_done.result_message.".to_string());
    lines.push(String::new());

    lines.push("## Session".to_string());
    lines.push(format!("session_id: {}", snapshot.session_id));
    lines.push(format!("turn_id: {}", snapshot.turn_id));
    lines.push(String::new());

    lines.push("## System Context (authoritative)".to_string());
    lines.push(format!(
        "runtime_version: {}",
        snapshot.system_context.runtime_version
    ));
    lines.push(format!(
        "workspace_root: {}",
        snapshot.system_context.workspace_root
    ));
    lines.push("path_policy.managed_uri_patterns:".to_string());
    for pattern in &snapshot.system_context.path_policy.managed_uri_patterns {
        lines.push(format!("- {pattern}"));
    }
    lines.push(format!(
        "path_policy.fs_uri_policy: {}",
        snapshot.system_context.path_policy.fs_uri_policy
    ));
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
    lines.push("tool_policy:".to_string());
    lines.push(format!(
        "- non_triggering_tools: {}",
        snapshot
            .system_context
            .tool_policy
            .non_triggering_tools
            .join(",")
    ));
    lines.push(format!(
        "- general_tools_trigger_followup_turn: {}",
        snapshot
            .system_context
            .tool_policy
            .general_tools_trigger_followup_turn
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
        "- lookup_tool: {}",
        snapshot.system_context.history_policy.lookup_tool
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
