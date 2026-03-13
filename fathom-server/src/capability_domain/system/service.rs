use std::future::Future;
use std::pin::Pin;

use crate::session::inspection::{
    ExecutionInspection, ExecutionListPage, ExecutionListQuery, PayloadSlice,
};

pub(crate) type SystemInspectionFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SystemInspectionError>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub(crate) enum SystemInspectionError {
    Input(String),
    Runtime(String),
}

pub(crate) trait SystemInspectionService: Send + Sync + 'static {
    fn list_executions<'a>(
        &'a self,
        session_id: &'a str,
        query: ExecutionListQuery,
    ) -> SystemInspectionFuture<'a, ExecutionListPage>;

    fn get_execution<'a>(
        &'a self,
        session_id: &'a str,
        execution_id: &'a str,
    ) -> SystemInspectionFuture<'a, Option<ExecutionInspection>>;

    fn read_execution_input<'a>(
        &'a self,
        session_id: &'a str,
        execution_id: &'a str,
        offset: usize,
        limit: usize,
    ) -> SystemInspectionFuture<'a, PayloadSlice>;

    fn read_execution_result<'a>(
        &'a self,
        session_id: &'a str,
        execution_id: &'a str,
        offset: usize,
        limit: usize,
    ) -> SystemInspectionFuture<'a, PayloadSlice>;
}

#[cfg(test)]
pub(crate) struct UnavailableSystemInspectionService;

#[cfg(test)]
impl SystemInspectionService for UnavailableSystemInspectionService {
    fn list_executions<'a>(
        &'a self,
        _session_id: &'a str,
        _query: ExecutionListQuery,
    ) -> SystemInspectionFuture<'a, ExecutionListPage> {
        Box::pin(async {
            Err(SystemInspectionError::Runtime(
                "system inspection service is unavailable".to_string(),
            ))
        })
    }

    fn get_execution<'a>(
        &'a self,
        _session_id: &'a str,
        _execution_id: &'a str,
    ) -> SystemInspectionFuture<'a, Option<ExecutionInspection>> {
        Box::pin(async {
            Err(SystemInspectionError::Runtime(
                "system inspection service is unavailable".to_string(),
            ))
        })
    }

    fn read_execution_input<'a>(
        &'a self,
        _session_id: &'a str,
        _execution_id: &'a str,
        _offset: usize,
        _limit: usize,
    ) -> SystemInspectionFuture<'a, PayloadSlice> {
        Box::pin(async {
            Err(SystemInspectionError::Runtime(
                "system inspection service is unavailable".to_string(),
            ))
        })
    }

    fn read_execution_result<'a>(
        &'a self,
        _session_id: &'a str,
        _execution_id: &'a str,
        _offset: usize,
        _limit: usize,
    ) -> SystemInspectionFuture<'a, PayloadSlice> {
        Box::pin(async {
            Err(SystemInspectionError::Runtime(
                "system inspection service is unavailable".to_string(),
            ))
        })
    }
}
