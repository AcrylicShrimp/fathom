use tokio::sync::mpsc;

use crate::session::state::SessionCommand;

use fathom_capability_domain::{
    CapabilityActionKey, CapabilityActionResult, CapabilityActionSubmission, DomainInstance,
};
use serde_json::Value;

const ACTION_BACKGROUND_KEY: &str = "background";

#[derive(Debug, Clone)]
pub(crate) struct CapabilityDomainCommittedExecution {
    pub(crate) execution_id: String,
    pub(crate) result: CapabilityActionResult,
}

#[derive(Debug, Clone)]
pub(crate) struct CapabilityDomainCommittedAction {
    pub(crate) submission_id: String,
    pub(crate) capability_domain_id: String,
    pub(crate) executions: Vec<CapabilityDomainCommittedExecution>,
}

#[derive(Clone)]
pub(crate) struct CapabilityDomainActorHandle {
    command_tx: mpsc::Sender<CapabilityDomainActionSubmission>,
}

impl CapabilityDomainActorHandle {
    pub(crate) async fn submit(&self, submission: CapabilityDomainActionSubmission) {
        let _ = self.command_tx.send(submission).await;
    }
}

#[derive(Clone)]
pub(crate) struct CapabilityDomainActionSubmission {
    pub(crate) submission_id: String,
    pub(crate) executions: Vec<CapabilityDomainActionExecution>,
}

#[derive(Clone)]
pub(crate) struct CapabilityDomainActionExecution {
    pub(crate) execution_id: String,
    pub(crate) action_key: CapabilityActionKey,
    pub(crate) args_json: String,
}

pub(crate) fn spawn_capability_domain_actor(
    capability_domain_id: String,
    mut domain_instance: Box<dyn DomainInstance>,
    session_command_tx: mpsc::Sender<SessionCommand>,
) -> CapabilityDomainActorHandle {
    let (command_tx, mut command_rx) = mpsc::channel::<CapabilityDomainActionSubmission>(128);
    let handle = CapabilityDomainActorHandle {
        command_tx: command_tx.clone(),
    };

    tokio::spawn(async move {
        while let Some(submission) = command_rx.recv().await {
            let executions = execute_submission(&mut *domain_instance, &submission).await;
            let committed = CapabilityDomainCommittedAction {
                submission_id: submission.submission_id,
                capability_domain_id: capability_domain_id.clone(),
                executions,
            };
            let _ = session_command_tx
                .send(SessionCommand::CapabilityDomainActionCommitted { committed })
                .await;
        }
    });

    handle
}

async fn execute_submission(
    domain_instance: &mut dyn DomainInstance,
    submission: &CapabilityDomainActionSubmission,
) -> Vec<CapabilityDomainCommittedExecution> {
    let mut prepared_actions = Vec::new();
    let mut results = vec![None; submission.executions.len()];

    for (index, execution) in submission.executions.iter().enumerate() {
        match parse_submission_args(&execution.args_json) {
            Ok(args) => prepared_actions.push((
                index,
                CapabilityActionSubmission {
                    action_key: execution.action_key,
                    args,
                },
            )),
            Err(error) => results[index] = Some(error),
        }
    }

    if !prepared_actions.is_empty() {
        let domain_results = domain_instance
            .execute_actions(
                prepared_actions
                    .iter()
                    .map(|(_, submission)| submission.clone())
                    .collect(),
            )
            .await;

        if domain_results.len() != prepared_actions.len() {
            let error = CapabilityActionResult::runtime_error(
                "invalid_result_count",
                format!(
                    "capability domain returned {} results for {} submitted actions",
                    domain_results.len(),
                    prepared_actions.len()
                ),
                None,
                0,
            );
            for (index, _) in &prepared_actions {
                results[*index] = Some(error.clone());
            }
        } else {
            for ((index, _), result) in prepared_actions.into_iter().zip(domain_results) {
                results[index] = Some(result);
            }
        }
    }

    submission
        .executions
        .iter()
        .enumerate()
        .map(|(index, execution)| CapabilityDomainCommittedExecution {
            execution_id: execution.execution_id.clone(),
            result: results[index].clone().unwrap_or_else(|| {
                CapabilityActionResult::runtime_error(
                    "missing_execution_result",
                    "capability domain execution produced no result",
                    None,
                    0,
                )
            }),
        })
        .collect()
}

fn parse_submission_args(args_json: &str) -> Result<Value, CapabilityActionResult> {
    let mut value: Value = serde_json::from_str(args_json).map_err(|error| {
        CapabilityActionResult::input_error(
            "invalid_args_json",
            format!("failed to parse action arguments: {error}"),
            None,
            0,
        )
    })?;
    let Value::Object(object) = &mut value else {
        return Err(CapabilityActionResult::input_error(
            "invalid_args",
            "action arguments must be a JSON object",
            None,
            0,
        ));
    };
    object.remove(ACTION_BACKGROUND_KEY);
    Ok(value)
}
