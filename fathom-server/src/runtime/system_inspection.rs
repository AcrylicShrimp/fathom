use std::sync::Weak;

use tokio::sync::oneshot;

use crate::capability_domain::{
    SystemInspectionError, SystemInspectionFuture, SystemInspectionService,
};
use crate::runtime::RuntimeInner;
use crate::session::inspection::{
    ExecutionInspection, ExecutionListPage, ExecutionListQuery, PayloadSlice,
};
use crate::session::{SessionCommand, SessionRuntime};

pub(crate) struct RuntimeSystemInspectionService {
    inner: Weak<RuntimeInner>,
}

impl RuntimeSystemInspectionService {
    pub(crate) fn new(inner: Weak<RuntimeInner>) -> Self {
        Self { inner }
    }

    async fn session(&self, session_id: &str) -> Result<SessionRuntime, SystemInspectionError> {
        let inner = self
            .inner
            .upgrade()
            .ok_or_else(|| SystemInspectionError::Runtime("runtime is unavailable".to_string()))?;
        inner
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                SystemInspectionError::Input(format!("session `{session_id}` not found"))
            })
    }
}

impl SystemInspectionService for RuntimeSystemInspectionService {
    fn list_executions<'a>(
        &'a self,
        session_id: &'a str,
        query: ExecutionListQuery,
    ) -> SystemInspectionFuture<'a, ExecutionListPage> {
        Box::pin(async move {
            let session = self.session(session_id).await?;
            let (response_tx, response_rx) = oneshot::channel();
            session
                .command_tx
                .send(SessionCommand::InspectListExecutions {
                    query,
                    respond_to: response_tx,
                })
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session actor unavailable".to_string())
                })?;
            response_rx
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session inspection unavailable".to_string())
                })?
                .map_err(SystemInspectionError::Input)
        })
    }

    fn get_execution<'a>(
        &'a self,
        session_id: &'a str,
        execution_id: &'a str,
    ) -> SystemInspectionFuture<'a, Option<ExecutionInspection>> {
        Box::pin(async move {
            let session = self.session(session_id).await?;
            let (response_tx, response_rx) = oneshot::channel();
            session
                .command_tx
                .send(SessionCommand::InspectGetExecution {
                    execution_id: execution_id.to_string(),
                    respond_to: response_tx,
                })
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session actor unavailable".to_string())
                })?;
            response_rx
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session inspection unavailable".to_string())
                })?
                .map_err(SystemInspectionError::Input)
        })
    }

    fn read_execution_input<'a>(
        &'a self,
        session_id: &'a str,
        execution_id: &'a str,
        offset: usize,
        limit: usize,
    ) -> SystemInspectionFuture<'a, PayloadSlice> {
        Box::pin(async move {
            let session = self.session(session_id).await?;
            let (response_tx, response_rx) = oneshot::channel();
            session
                .command_tx
                .send(SessionCommand::InspectReadExecutionInput {
                    execution_id: execution_id.to_string(),
                    offset,
                    limit,
                    respond_to: response_tx,
                })
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session actor unavailable".to_string())
                })?;
            response_rx
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session inspection unavailable".to_string())
                })?
                .map_err(SystemInspectionError::Input)
        })
    }

    fn read_execution_result<'a>(
        &'a self,
        session_id: &'a str,
        execution_id: &'a str,
        offset: usize,
        limit: usize,
    ) -> SystemInspectionFuture<'a, PayloadSlice> {
        Box::pin(async move {
            let session = self.session(session_id).await?;
            let (response_tx, response_rx) = oneshot::channel();
            session
                .command_tx
                .send(SessionCommand::InspectReadExecutionResult {
                    execution_id: execution_id.to_string(),
                    offset,
                    limit,
                    respond_to: response_tx,
                })
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session actor unavailable".to_string())
                })?;
            response_rx
                .await
                .map_err(|_| {
                    SystemInspectionError::Runtime("session inspection unavailable".to_string())
                })?
                .map_err(SystemInspectionError::Input)
        })
    }
}
