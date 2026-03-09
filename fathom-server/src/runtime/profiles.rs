use tonic::Status;

use super::Runtime;
use crate::util::{default_agent_profile, default_user_profile, now_unix_ms};
use fathom_protocol::pb;

impl Runtime {
    pub(crate) async fn get_or_create_user_profile(&self, user_id: &str) -> pb::UserProfile {
        let mut profiles = self.inner.user_profiles.write().await;
        let profile = profiles
            .entry(user_id.to_string())
            .or_insert_with(|| default_user_profile(user_id));
        profile.clone()
    }

    pub(crate) async fn get_or_create_agent_profile(&self, agent_id: &str) -> pb::AgentProfile {
        let mut profiles = self.inner.agent_profiles.write().await;
        let profile = profiles
            .entry(agent_id.to_string())
            .or_insert_with(|| default_agent_profile(agent_id));
        profile.clone()
    }

    pub(crate) async fn list_user_profiles(&self) -> Vec<pb::UserProfile> {
        let mut profiles = self
            .inner
            .user_profiles
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|a, b| a.user_id.cmp(&b.user_id));
        profiles
    }

    pub(crate) async fn list_agent_profiles(&self) -> Vec<pb::AgentProfile> {
        let mut profiles = self
            .inner
            .agent_profiles
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
        profiles
    }

    pub(crate) async fn upsert_user_profile(
        &self,
        mut profile: pb::UserProfile,
    ) -> Result<pb::UserProfile, Status> {
        if profile.user_id.trim().is_empty() {
            return Err(Status::invalid_argument("profile.user_id is required"));
        }
        if profile.updated_at_unix_ms == 0 {
            profile.updated_at_unix_ms = now_unix_ms();
        }

        self.inner
            .user_profiles
            .write()
            .await
            .insert(profile.user_id.clone(), profile.clone());
        Ok(profile)
    }

    pub(crate) async fn upsert_agent_profile(
        &self,
        mut profile: pb::AgentProfile,
    ) -> Result<pb::AgentProfile, Status> {
        if profile.agent_id.trim().is_empty() {
            return Err(Status::invalid_argument("profile.agent_id is required"));
        }

        let mut profiles = self.inner.agent_profiles.write().await;
        let current_version = profiles
            .get(&profile.agent_id)
            .map(|current| current.spec_version)
            .unwrap_or(0);
        if profile.spec_version == 0 {
            profile.spec_version = current_version.max(1) + 1;
        }
        if profile.updated_at_unix_ms == 0 {
            profile.updated_at_unix_ms = now_unix_ms();
        }

        profiles.insert(profile.agent_id.clone(), profile.clone());
        Ok(profile)
    }

    pub(crate) async fn fetch_agent_profile(&self, agent_id: &str) -> Option<pb::AgentProfile> {
        self.inner
            .agent_profiles
            .read()
            .await
            .get(agent_id)
            .cloned()
    }

    pub(crate) async fn fetch_user_profile(&self, user_id: &str) -> Option<pb::UserProfile> {
        self.inner.user_profiles.read().await.get(user_id).cloned()
    }
}
