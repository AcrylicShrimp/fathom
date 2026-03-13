use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::Status;

use super::session_setup::{
    DefaultSessionSetupPolicy, RuntimeSessionSetupContext, SessionSetupPolicy, SessionSetupRequest,
    build_session_state,
};
use super::{EVENT_BUFFER_SIZE, Runtime, SESSION_CMD_BUFFER_SIZE};
use crate::session::{SessionCommand, SessionRuntime, run_session_actor};
use fathom_protocol::pb;

impl Runtime {
    pub(crate) async fn create_session(
        &self,
        agent_id: String,
        participant_user_ids: Vec<String>,
    ) -> Result<pb::SessionSummary, Status> {
        let setup_policy = DefaultSessionSetupPolicy::new(self.capability_domain_registry());
        let setup_context = RuntimeSessionSetupContext::new(self);
        let setup = setup_policy
            .resolve(
                &setup_context,
                SessionSetupRequest {
                    agent_id,
                    participant_user_ids,
                },
            )
            .await?;
        let session_id = setup.session_id.clone();
        let state = build_session_state(setup);
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
