use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::pb;

pub(crate) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as i64
}

pub(crate) fn dedup_ids(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() {
            continue;
        }
        if seen.insert(id.clone()) {
            deduped.push(id);
        }
    }
    deduped
}

pub(crate) fn default_user_profile(user_id: &str) -> pb::UserProfile {
    pb::UserProfile {
        user_id: user_id.to_string(),
        name: user_id.to_string(),
        nickname: user_id.to_string(),
        preferences_json: "{}".to_string(),
        user_md: format!("# USER.md\\n\\nid: {user_id}\\n"),
        long_term_memory_md: "# Long-Term User Memory\\n".to_string(),
        updated_at_unix_ms: now_unix_ms(),
    }
}

pub(crate) fn default_agent_profile(agent_id: &str) -> pb::AgentProfile {
    pb::AgentProfile {
        agent_id: agent_id.to_string(),
        display_name: "Fathom".to_string(),
        agents_md: "# AGENTS.md\\n\\nFollow repository and runtime rules.\\n".to_string(),
        soul_md: "# SOUL.md\\n\\nPragmatic, clear, direct.\\n".to_string(),
        identity_md: format!("# IDENTITY.md\\n\\nid: {agent_id}\\n"),
        guidelines_md: "# Guidelines\\n\\nBe deterministic.\\n".to_string(),
        code_of_conduct_md: "# Code Of Conduct\\n\\nNo harmful actions.\\n".to_string(),
        long_term_memory_md: "# Long-Term Agent Memory\\n".to_string(),
        spec_version: 1,
        updated_at_unix_ms: now_unix_ms(),
    }
}

pub(crate) fn task_status_label(status: pb::TaskStatus) -> &'static str {
    match status {
        pb::TaskStatus::Unspecified => "unspecified",
        pb::TaskStatus::Pending => "pending",
        pb::TaskStatus::Running => "running",
        pb::TaskStatus::Succeeded => "succeeded",
        pb::TaskStatus::Failed => "failed",
        pb::TaskStatus::Canceled => "canceled",
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
