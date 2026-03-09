use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::profile_material::{default_agent_material_json, default_user_material_json};
use fathom_protocol::pb;

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
        material_json: default_user_material_json(user_id),
        updated_at_unix_ms: now_unix_ms(),
    }
}

pub(crate) fn default_agent_profile(agent_id: &str) -> pb::AgentProfile {
    pb::AgentProfile {
        agent_id: agent_id.to_string(),
        display_name: "Fathom".to_string(),
        material_json: default_agent_material_json(agent_id),
        spec_version: 1,
        updated_at_unix_ms: now_unix_ms(),
    }
}
