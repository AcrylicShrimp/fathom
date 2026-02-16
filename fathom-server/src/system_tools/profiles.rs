use serde_json::{Value, json};

use crate::pb;
use crate::runtime::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileKind {
    Agent,
    User,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileView {
    Summary,
    Full,
}

pub(crate) fn parse_profile_kind(raw: &str) -> Result<ProfileKind, String> {
    match raw {
        "agent" => Ok(ProfileKind::Agent),
        "user" => Ok(ProfileKind::User),
        "all" => Ok(ProfileKind::All),
        _ => Err("kind must be `agent`, `user`, or `all`".to_string()),
    }
}

pub(crate) fn parse_profile_view(raw: &str) -> Result<ProfileView, String> {
    match raw {
        "summary" => Ok(ProfileView::Summary),
        "full" => Ok(ProfileView::Full),
        _ => Err("view must be `summary` or `full`".to_string()),
    }
}

pub(crate) async fn list_profiles(runtime: &Runtime, kind: ProfileKind) -> Value {
    let agents = runtime.list_agent_profiles().await;
    let users = runtime.list_user_profiles().await;

    match kind {
        ProfileKind::Agent => {
            json!({"agents": agents.into_iter().map(agent_summary).collect::<Vec<_>>()})
        }
        ProfileKind::User => {
            json!({"users": users.into_iter().map(user_summary).collect::<Vec<_>>()})
        }
        ProfileKind::All => json!({
            "agents": agents.into_iter().map(agent_summary).collect::<Vec<_>>(),
            "users": users.into_iter().map(user_summary).collect::<Vec<_>>(),
        }),
    }
}

pub(crate) async fn get_profile(
    runtime: &Runtime,
    kind: ProfileKind,
    id: &str,
    view: ProfileView,
) -> Result<Value, String> {
    match kind {
        ProfileKind::Agent => {
            let profile = runtime.get_or_create_agent_profile(id).await;
            Ok(match view {
                ProfileView::Summary => json!({ "profile": agent_summary(profile) }),
                ProfileView::Full => json!({ "profile": agent_full(profile) }),
            })
        }
        ProfileKind::User => {
            let profile = runtime.get_or_create_user_profile(id).await;
            Ok(match view {
                ProfileView::Summary => json!({ "profile": user_summary(profile) }),
                ProfileView::Full => json!({ "profile": user_full(profile) }),
            })
        }
        ProfileKind::All => Err("kind must be `agent` or `user` for sys_get_profile".to_string()),
    }
}

fn agent_summary(profile: pb::AgentProfile) -> Value {
    json!({
        "agent_id": profile.agent_id,
        "display_name": profile.display_name,
        "spec_version": profile.spec_version,
        "updated_at_unix_ms": profile.updated_at_unix_ms,
    })
}

fn user_summary(profile: pb::UserProfile) -> Value {
    json!({
        "user_id": profile.user_id,
        "name": profile.name,
        "nickname": profile.nickname,
        "updated_at_unix_ms": profile.updated_at_unix_ms,
    })
}

fn agent_full(profile: pb::AgentProfile) -> Value {
    json!({
        "agent_id": profile.agent_id,
        "display_name": profile.display_name,
        "agents_md": profile.agents_md,
        "soul_md": profile.soul_md,
        "identity_md": profile.identity_md,
        "guidelines_md": profile.guidelines_md,
        "code_of_conduct_md": profile.code_of_conduct_md,
        "long_term_memory_md": profile.long_term_memory_md,
        "spec_version": profile.spec_version,
        "updated_at_unix_ms": profile.updated_at_unix_ms,
    })
}

fn user_full(profile: pb::UserProfile) -> Value {
    json!({
        "user_id": profile.user_id,
        "name": profile.name,
        "nickname": profile.nickname,
        "preferences_json": profile.preferences_json,
        "user_md": profile.user_md,
        "long_term_memory_md": profile.long_term_memory_md,
        "updated_at_unix_ms": profile.updated_at_unix_ms,
    })
}
