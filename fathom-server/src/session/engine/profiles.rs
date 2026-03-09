use crate::runtime::Runtime;
use crate::session::state::SessionState;
use fathom_protocol::pb;

pub(super) async fn apply_profile_refresh(
    runtime: &Runtime,
    state: &mut SessionState,
    refresh: &pb::RefreshProfileTrigger,
) -> Vec<String> {
    let scope = pb::RefreshScope::try_from(refresh.scope).unwrap_or(pb::RefreshScope::All);
    let mut refreshed_user_ids = Vec::new();

    if matches!(scope, pb::RefreshScope::Agent | pb::RefreshScope::All)
        && let Some(profile) = runtime.fetch_agent_profile(&state.agent_id).await
    {
        state.agent_profile_copy = profile;
    }

    if matches!(scope, pb::RefreshScope::User | pb::RefreshScope::All) {
        if scope == pb::RefreshScope::User && !refresh.user_id.trim().is_empty() {
            if let Some(profile) = runtime.fetch_user_profile(&refresh.user_id).await {
                state
                    .participant_user_profiles_copy
                    .insert(refresh.user_id.clone(), profile);
                refreshed_user_ids.push(refresh.user_id.clone());
            }
        } else {
            for user_id in &state.participant_user_ids {
                if let Some(profile) = runtime.fetch_user_profile(user_id).await {
                    state
                        .participant_user_profiles_copy
                        .insert(user_id.clone(), profile);
                    refreshed_user_ids.push(user_id.clone());
                }
            }
        }
    }

    refreshed_user_ids
}
