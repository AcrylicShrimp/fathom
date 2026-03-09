use std::collections::HashMap;

use serde_json::json;
use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::Status;

use super::{EVENT_BUFFER_SIZE, Runtime, SESSION_CMD_BUFFER_SIZE};
use crate::environment::EnvironmentRegistry;
use crate::pb;
use crate::session::{SessionCommand, SessionRuntime, SessionState, run_session_actor};
use crate::util::dedup_ids;

impl Runtime {
    pub(crate) async fn create_session(
        &self,
        agent_id: String,
        participant_user_ids: Vec<String>,
    ) -> Result<pb::SessionSummary, Status> {
        if agent_id.trim().is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let participant_user_ids = dedup_ids(participant_user_ids);
        let agent_profile_copy = self.get_or_create_agent_profile(&agent_id).await;
        let mut participant_user_profiles_copy = HashMap::new();

        for user_id in &participant_user_ids {
            let profile = self.get_or_create_user_profile(user_id).await;
            participant_user_profiles_copy.insert(user_id.clone(), profile);
        }

        let session_id = self.next_session_id();
        let engaged_environment_ids = EnvironmentRegistry::default_engaged_environment_ids()
            .into_iter()
            .collect();
        let mut environment_snapshots = EnvironmentRegistry::initial_environment_snapshots()
            .into_iter()
            .collect::<HashMap<_, _>>();
        let base_path = self.workspace_root().display().to_string();
        for environment_id in [
            fathom_env_fs::FILESYSTEM_ENVIRONMENT_ID,
            fathom_env_shell::SHELL_ENVIRONMENT_ID,
        ] {
            if let Some(snapshot) = environment_snapshots.get_mut(environment_id) {
                if let Some(state) = snapshot.state_json.as_object_mut() {
                    state.insert("base_path".to_string(), json!(base_path));
                } else {
                    snapshot.state_json = json!({
                        "base_path": base_path
                    });
                }
            }
        }
        let state = SessionState::new(
            session_id.clone(),
            agent_id,
            participant_user_ids,
            agent_profile_copy,
            participant_user_profiles_copy,
            engaged_environment_ids,
            environment_snapshots,
        );
        let session_summary = state.to_summary();

        let (events_tx, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        let (command_tx, command_rx) = mpsc::channel(SESSION_CMD_BUFFER_SIZE);

        tokio::spawn(run_session_actor(
            self.clone(),
            state,
            command_tx.clone(),
            command_rx,
            events_tx.clone(),
        ));

        self.inner.sessions.write().await.insert(
            session_id,
            SessionRuntime {
                command_tx,
                events_tx,
            },
        );

        Ok(session_summary)
    }

    pub(crate) async fn list_sessions(&self) -> Result<Vec<pb::SessionSummary>, Status> {
        let sessions = self
            .inner
            .sessions
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let mut summaries = Vec::with_capacity(sessions.len());
        for session in sessions {
            let (response_tx, response_rx) = oneshot::channel();
            session
                .command_tx
                .send(SessionCommand::GetSummary {
                    respond_to: response_tx,
                })
                .await
                .map_err(|_| Status::unavailable("session actor unavailable"))?;
            let summary = response_rx
                .await
                .map_err(|_| Status::unavailable("session summary unavailable"))?;
            summaries.push(summary);
        }

        summaries.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(summaries)
    }

    pub(crate) async fn get_session(&self, session_id: &str) -> Result<SessionRuntime, Status> {
        self.inner
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| Status::not_found("session not found"))
    }

    pub(crate) async fn enqueue_trigger(
        &self,
        session_id: &str,
        trigger: pb::Trigger,
    ) -> Result<pb::EnqueueTriggerResponse, Status> {
        let session = self.get_session(session_id).await?;
        let (response_tx, response_rx) = oneshot::channel();
        session
            .command_tx
            .send(SessionCommand::EnqueueTrigger {
                trigger,
                respond_to: response_tx,
            })
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?;
        response_rx
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?
    }

    pub(crate) async fn list_executions(
        &self,
        session_id: &str,
    ) -> Result<Vec<pb::Execution>, Status> {
        let session = self.get_session(session_id).await?;
        let (response_tx, response_rx) = oneshot::channel();
        session
            .command_tx
            .send(SessionCommand::ListExecutions {
                respond_to: response_tx,
            })
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?;
        response_rx
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))
    }

    pub(crate) async fn cancel_execution(
        &self,
        session_id: &str,
        execution_id: String,
    ) -> Result<pb::CancelExecutionResponse, Status> {
        let session = self.get_session(session_id).await?;
        let (response_tx, response_rx) = oneshot::channel();
        session
            .command_tx
            .send(SessionCommand::CancelExecution {
                execution_id,
                respond_to: response_tx,
            })
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?;
        response_rx
            .await
            .map_err(|_| Status::unavailable("session actor unavailable"))?
    }
}
