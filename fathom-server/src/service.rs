use std::path::PathBuf;
use std::pin::Pin;

use anyhow::Result;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::pb;
use crate::pb::runtime_service_server::RuntimeService;
use crate::runtime::{DEFAULT_TASK_CAPACITY, DEFAULT_TASK_RUNTIME_MS, Runtime};
use crate::util::now_unix_ms;

#[derive(Clone)]
pub struct FathomRuntimeService {
    runtime: Runtime,
}

impl Default for FathomRuntimeService {
    fn default() -> Self {
        Self {
            runtime: Runtime::new(DEFAULT_TASK_CAPACITY, DEFAULT_TASK_RUNTIME_MS),
        }
    }
}

impl FathomRuntimeService {
    pub fn with_workspace_root(workspace_root: PathBuf) -> Result<Self> {
        Ok(Self {
            runtime: Runtime::new_with_workspace_root(
                DEFAULT_TASK_CAPACITY,
                DEFAULT_TASK_RUNTIME_MS,
                workspace_root,
            )?,
        })
    }
}

#[tonic::async_trait]
impl RuntimeService for FathomRuntimeService {
    type AttachSessionEventsStream =
        Pin<Box<dyn Stream<Item = Result<pb::SessionEvent, Status>> + Send + 'static>>;

    async fn create_session(
        &self,
        request: Request<pb::CreateSessionRequest>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        let request = request.into_inner();
        let session = self
            .runtime
            .create_session(request.agent_id, request.participant_user_ids)
            .await?;
        Ok(Response::new(pb::CreateSessionResponse {
            session: Some(session),
        }))
    }

    async fn list_sessions(
        &self,
        _request: Request<pb::ListSessionsRequest>,
    ) -> Result<Response<pb::ListSessionsResponse>, Status> {
        let sessions = self.runtime.list_sessions().await?;
        Ok(Response::new(pb::ListSessionsResponse { sessions }))
    }

    async fn enqueue_trigger(
        &self,
        request: Request<pb::EnqueueTriggerRequest>,
    ) -> Result<Response<pb::EnqueueTriggerResponse>, Status> {
        let request = request.into_inner();
        if request.session_id.trim().is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }

        let trigger = request
            .trigger
            .ok_or_else(|| Status::invalid_argument("trigger is required"))?;
        let trigger = normalize_trigger(trigger, &self.runtime)?;

        let response = self
            .runtime
            .enqueue_trigger(&request.session_id, trigger)
            .await?;
        Ok(Response::new(response))
    }

    async fn attach_session_events(
        &self,
        request: Request<pb::AttachSessionEventsRequest>,
    ) -> Result<Response<Self::AttachSessionEventsStream>, Status> {
        let request = request.into_inner();
        if request.session_id.trim().is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }

        let session = self.runtime.get_session(&request.session_id).await?;
        let stream = BroadcastStream::new(session.events_tx.subscribe()).map(|event| match event {
            Ok(event) => Ok(event),
            Err(BroadcastStreamRecvError::Lagged(skipped)) => Err(Status::resource_exhausted(
                format!("event stream lagged by {skipped} event(s)"),
            )),
        });
        Ok(Response::new(Box::pin(stream)))
    }

    async fn list_tasks(
        &self,
        request: Request<pb::ListTasksRequest>,
    ) -> Result<Response<pb::ListTasksResponse>, Status> {
        let request = request.into_inner();
        if request.session_id.trim().is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        let tasks = self.runtime.list_tasks(&request.session_id).await?;
        Ok(Response::new(pb::ListTasksResponse { tasks }))
    }

    async fn cancel_task(
        &self,
        request: Request<pb::CancelTaskRequest>,
    ) -> Result<Response<pb::CancelTaskResponse>, Status> {
        let request = request.into_inner();
        if request.session_id.trim().is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        if request.task_id.trim().is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }
        let response = self
            .runtime
            .cancel_task(&request.session_id, request.task_id)
            .await?;
        Ok(Response::new(response))
    }

    async fn get_user_profile(
        &self,
        request: Request<pb::GetUserProfileRequest>,
    ) -> Result<Response<pb::GetUserProfileResponse>, Status> {
        let request = request.into_inner();
        if request.user_id.trim().is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }
        let profile = self
            .runtime
            .get_or_create_user_profile(&request.user_id)
            .await;
        Ok(Response::new(pb::GetUserProfileResponse {
            profile: Some(profile),
        }))
    }

    async fn upsert_user_profile(
        &self,
        request: Request<pb::UpsertUserProfileRequest>,
    ) -> Result<Response<pb::UpsertUserProfileResponse>, Status> {
        let profile = request
            .into_inner()
            .profile
            .ok_or_else(|| Status::invalid_argument("profile is required"))?;
        let profile = self.runtime.upsert_user_profile(profile).await?;
        Ok(Response::new(pb::UpsertUserProfileResponse {
            profile: Some(profile),
        }))
    }

    async fn get_agent_profile(
        &self,
        request: Request<pb::GetAgentProfileRequest>,
    ) -> Result<Response<pb::GetAgentProfileResponse>, Status> {
        let request = request.into_inner();
        if request.agent_id.trim().is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }
        let profile = self
            .runtime
            .get_or_create_agent_profile(&request.agent_id)
            .await;
        Ok(Response::new(pb::GetAgentProfileResponse {
            profile: Some(profile),
        }))
    }

    async fn upsert_agent_profile(
        &self,
        request: Request<pb::UpsertAgentProfileRequest>,
    ) -> Result<Response<pb::UpsertAgentProfileResponse>, Status> {
        let profile = request
            .into_inner()
            .profile
            .ok_or_else(|| Status::invalid_argument("profile is required"))?;
        let profile = self.runtime.upsert_agent_profile(profile).await?;
        Ok(Response::new(pb::UpsertAgentProfileResponse {
            profile: Some(profile),
        }))
    }
}

fn normalize_trigger(trigger: pb::Trigger, runtime: &Runtime) -> Result<pb::Trigger, Status> {
    if trigger.kind.is_none() {
        return Err(Status::invalid_argument("trigger.kind is required"));
    }
    let mut trigger = trigger;
    if trigger.trigger_id.trim().is_empty() {
        trigger.trigger_id = runtime.next_trigger_id();
    }
    if trigger.created_at_unix_ms == 0 {
        trigger.created_at_unix_ms = now_unix_ms();
    }
    Ok(trigger)
}
