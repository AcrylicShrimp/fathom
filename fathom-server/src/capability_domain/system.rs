mod common;
mod get_execution;
mod list_executions;
mod payload;
mod read_execution_input;
mod read_execution_result;
mod service;

use std::sync::Arc;
use std::time::Instant;

use fathom_capability_domain::{
    CapabilityActionDefinition, CapabilityActionResult, CapabilityActionSubmission,
    CapabilityDomainRecipe, CapabilityDomainSessionContext, CapabilityDomainSpec, DomainFactory,
    DomainInstance, DomainInstanceFuture,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::session::inspection::{
    ExecutionInspection, ExecutionInspectionState, ExecutionListQuery,
};

use common::SYSTEM_CAPABILITY_DOMAIN_ID;
use payload::{preview_descriptor, slice_payload_response};
#[cfg(test)]
pub(crate) use service::UnavailableSystemInspectionService;
pub(crate) use service::{SystemInspectionError, SystemInspectionFuture, SystemInspectionService};

pub(crate) struct SystemDomainFactory {
    inspection_service: Arc<dyn SystemInspectionService>,
}

impl SystemDomainFactory {
    pub(super) fn new(inspection_service: Arc<dyn SystemInspectionService>) -> Self {
        Self { inspection_service }
    }
}

impl DomainFactory for SystemDomainFactory {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: SYSTEM_CAPABILITY_DOMAIN_ID,
            name: "System",
            description: "Privileged runtime inspection capability domain for current session execution state and execution payload access.",
            schema_version: 1,
        }
    }

    fn actions(&self) -> Vec<CapabilityActionDefinition> {
        vec![
            list_executions::definition(),
            get_execution::definition(),
            read_execution_input::definition(),
            read_execution_result::definition(),
        ]
    }

    fn create_instance(
        &self,
        session_context: CapabilityDomainSessionContext,
    ) -> Box<dyn DomainInstance> {
        Box::new(SystemDomainInstance {
            session_id: session_context.session_id,
            inspection_service: self.inspection_service.clone(),
        })
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Inspect recent executions".to_string(),
                steps: vec![
                    "Call `system__list_executions` to discover recent execution ids for the current session.".to_string(),
                    "Use the optional `state` or `action_id` filter when the list must stay narrow.".to_string(),
                    "Call `system__get_execution` on one id when you need its payload previews or final execution time.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Read execution input payload".to_string(),
                steps: vec![
                    "Start with `system__get_execution` to inspect the input preview and total size.".to_string(),
                    "Call `system__read_execution_input` with `execution_id`, `offset`, and `limit` to read a larger slice.".to_string(),
                    "Increase `offset` only when you need a later window from the same serialized payload.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Read execution result payload".to_string(),
                steps: vec![
                    "Call `system__get_execution` first to see whether the result payload exists yet.".to_string(),
                    "Call `system__read_execution_result` only after the execution has produced a result payload.".to_string(),
                    "Use bounded reads and move `offset` forward when the serialized result is larger than one slice.".to_string(),
                ],
            },
        ]
    }
}

struct SystemDomainInstance {
    session_id: String,
    inspection_service: Arc<dyn SystemInspectionService>,
}

impl DomainInstance for SystemDomainInstance {
    fn execute_actions<'a>(
        &'a mut self,
        submissions: Vec<CapabilityActionSubmission>,
    ) -> DomainInstanceFuture<'a> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(submissions.len());
            for submission in submissions {
                results.push(self.execute_submission(submission).await);
            }
            results
        })
    }
}

impl SystemDomainInstance {
    async fn execute_submission(
        &self,
        submission: CapabilityActionSubmission,
    ) -> CapabilityActionResult {
        let Some(action_name) = action_name_for_key(submission.action_key) else {
            return CapabilityActionResult::runtime_error(
                "unknown_action_key",
                format!(
                    "system domain instance does not recognize action key {}",
                    submission.action_key.0
                ),
                None,
                0,
            );
        };
        let started_at = Instant::now();
        let result = match action_name {
            "list_executions" => self.execute_list_executions(submission.args).await,
            "get_execution" => self.execute_get_execution(submission.args).await,
            "read_execution_input" => self.execute_read_execution_input(submission.args).await,
            "read_execution_result" => self.execute_read_execution_result(submission.args).await,
            _ => Err(SystemInspectionError::Runtime(format!(
                "system action `{action_name}` is not implemented"
            ))),
        };

        match result {
            Ok(payload) => CapabilityActionResult::success(
                payload,
                started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            ),
            Err(SystemInspectionError::Input(message)) => CapabilityActionResult::input_error(
                "invalid_args",
                message,
                None,
                started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            ),
            Err(SystemInspectionError::Runtime(message)) => CapabilityActionResult::runtime_error(
                "inspection_failed",
                message,
                None,
                started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            ),
        }
    }

    async fn execute_list_executions(&self, args: Value) -> Result<Value, SystemInspectionError> {
        let args = parse_args::<ListExecutionsArgs>(args, "system__list_executions")?;
        let query = ExecutionListQuery {
            cursor: args.cursor,
            limit: args
                .limit
                .map(u64_to_usize)
                .transpose()?
                .unwrap_or_default(),
            state: args.state.as_deref().map(parse_state_filter).transpose()?,
            action_id: args.action_id.filter(|value| !value.trim().is_empty()),
        };

        let page = self
            .inspection_service
            .list_executions(&self.session_id, query)
            .await?;

        Ok(json!({
            "executions": page.executions.into_iter().map(summary_to_json).collect::<Vec<_>>(),
            "next_cursor": page.next_cursor,
            "prev_cursor": page.prev_cursor,
        }))
    }

    async fn execute_get_execution(&self, args: Value) -> Result<Value, SystemInspectionError> {
        let args = parse_args::<GetExecutionArgs>(args, "system__get_execution")?;
        let execution_id = require_non_empty(args.execution_id, "execution_id")?;

        let execution = self
            .inspection_service
            .get_execution(&self.session_id, &execution_id)
            .await?
            .ok_or_else(|| {
                SystemInspectionError::Input(format!("execution `{execution_id}` not found"))
            })?;

        Ok(execution_to_json(execution))
    }

    async fn execute_read_execution_input(
        &self,
        args: Value,
    ) -> Result<Value, SystemInspectionError> {
        let args = parse_args::<ReadExecutionPayloadArgs>(args, "system__read_execution_input")?;
        let execution_id = require_non_empty(args.execution_id, "execution_id")?;
        let offset = args
            .offset
            .map(u64_to_usize)
            .transpose()?
            .unwrap_or_default();
        let limit = u64_to_usize(args.limit)?;

        let slice = self
            .inspection_service
            .read_execution_input(&self.session_id, &execution_id, offset, limit)
            .await?;

        Ok(slice_payload_response(
            slice.total_size,
            slice.offset,
            slice.limit,
            &slice.content,
        ))
    }

    async fn execute_read_execution_result(
        &self,
        args: Value,
    ) -> Result<Value, SystemInspectionError> {
        let args = parse_args::<ReadExecutionPayloadArgs>(args, "system__read_execution_result")?;
        let execution_id = require_non_empty(args.execution_id, "execution_id")?;
        let offset = args
            .offset
            .map(u64_to_usize)
            .transpose()?
            .unwrap_or_default();
        let limit = u64_to_usize(args.limit)?;

        let slice = self
            .inspection_service
            .read_execution_result(&self.session_id, &execution_id, offset, limit)
            .await?;

        Ok(slice_payload_response(
            slice.total_size,
            slice.offset,
            slice.limit,
            &slice.content,
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListExecutionsArgs {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    action_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetExecutionArgs {
    execution_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadExecutionPayloadArgs {
    execution_id: String,
    #[serde(default)]
    offset: Option<u64>,
    limit: u64,
}

fn action_name_for_key(key: fathom_capability_domain::CapabilityActionKey) -> Option<&'static str> {
    match key {
        common::SYSTEM_LIST_EXECUTIONS_ACTION_KEY => Some("list_executions"),
        common::SYSTEM_GET_EXECUTION_ACTION_KEY => Some("get_execution"),
        common::SYSTEM_READ_EXECUTION_INPUT_ACTION_KEY => Some("read_execution_input"),
        common::SYSTEM_READ_EXECUTION_RESULT_ACTION_KEY => Some("read_execution_result"),
        _ => None,
    }
}

fn parse_args<T>(args: Value, action_id: &str) -> Result<T, SystemInspectionError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(args).map_err(|error| {
        SystemInspectionError::Input(format!("failed to parse args for `{action_id}`: {error}"))
    })
}

fn parse_state_filter(raw: &str) -> Result<ExecutionInspectionState, SystemInspectionError> {
    ExecutionInspectionState::parse(raw).ok_or_else(|| {
        SystemInspectionError::Input(format!("invalid state `{raw}` for system execution filter"))
    })
}

fn u64_to_usize(value: u64) -> Result<usize, SystemInspectionError> {
    usize::try_from(value).map_err(|_| {
        SystemInspectionError::Input("numeric value is too large for this runtime".to_string())
    })
}

fn require_non_empty(value: String, field: &str) -> Result<String, SystemInspectionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SystemInspectionError::Input(format!(
            "`{field}` must be non-empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn summary_to_json(summary: crate::session::inspection::ExecutionSummary) -> Value {
    json!({
        "execution_id": summary.execution_id,
        "action_id": summary.action_id,
        "state": summary.state.as_str(),
    })
}

fn execution_to_json(execution: ExecutionInspection) -> Value {
    let mut object = Map::new();
    object.insert(
        "execution_id".to_string(),
        Value::String(execution.execution_id),
    );
    object.insert("action_id".to_string(), Value::String(execution.action_id));
    object.insert(
        "state".to_string(),
        Value::String(execution.state.as_str().to_string()),
    );
    object.insert(
        "input".to_string(),
        preview_descriptor(&execution.input_payload),
    );
    if let Some(result_payload) = execution.result_payload.as_deref() {
        object.insert("result".to_string(), preview_descriptor(result_payload));
    }
    if let Some(execution_time_ms) = execution.execution_time_ms {
        object.insert("execution_time_ms".to_string(), json!(execution_time_ms));
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{SystemDomainFactory, UnavailableSystemInspectionService, list_executions};
    use fathom_capability_domain::{
        ActionError, CapabilityActionSubmission, CapabilityDomainSessionContext, DomainFactory,
    };
    use serde_json::json;

    #[tokio::test]
    async fn system_factory_returns_runtime_error_when_service_is_unavailable() {
        let factory = SystemDomainFactory::new(Arc::new(UnavailableSystemInspectionService));
        let mut instance = factory.create_instance(CapabilityDomainSessionContext {
            session_id: "session-test".to_string(),
        });

        let results = instance
            .execute_actions(vec![CapabilityActionSubmission {
                action_key: list_executions::definition().key,
                args: json!({}),
            }])
            .await;

        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0].outcome,
            Err(ActionError::RuntimeError(error)) if error.code == "inspection_failed"
        ));
    }
}
