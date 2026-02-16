use futures_util::future::BoxFuture;

use crate::fs::TaskOutcome;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;

use fathom_env::{ActionHost, ActionOutcome};

pub(super) struct ServerActionHost<'a> {
    runtime: &'a Runtime,
    context: &'a TaskExecutionContext,
}

impl<'a> ServerActionHost<'a> {
    pub(super) fn new(runtime: &'a Runtime, context: &'a TaskExecutionContext) -> Self {
        Self { runtime, context }
    }
}

impl ActionHost for ServerActionHost<'_> {
    fn execute_environment_action<'a>(
        &'a self,
        environment_id: &'a str,
        action_name: &'a str,
        args_json: &'a str,
    ) -> BoxFuture<'a, Option<ActionOutcome>> {
        Box::pin(async move {
            let task_outcome = match environment_id {
                "filesystem" => {
                    crate::fs::execute_action(self.runtime, action_name, args_json).await
                }
                "system" => {
                    crate::system_env::execute_action(
                        self.runtime,
                        self.context,
                        action_name,
                        args_json,
                    )
                    .await
                }
                _ => None,
            };
            map_outcome(task_outcome)
        })
    }
}

fn map_outcome(outcome: Option<TaskOutcome>) -> Option<ActionOutcome> {
    outcome.map(|outcome| ActionOutcome {
        succeeded: outcome.succeeded,
        message: outcome.message,
        state_patch: None,
    })
}
