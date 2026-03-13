mod execute;
mod jina_read_url;
mod validate;

use std::time::Instant;

use fathom_capability_domain::{
    CapabilityActionDefinition, CapabilityActionResult, CapabilityActionSubmission,
    CapabilityDomainRecipe, CapabilityDomainSessionContext, CapabilityDomainSpec, DomainFactory,
    DomainInstance, DomainInstanceFuture,
};
use serde_json::Value;

pub const JINA_CAPABILITY_DOMAIN_ID: &str = "jina";
pub(crate) const JINA_ACTION_MAX_TIMEOUT_MS: u64 = 30_000;
pub(crate) const JINA_DEFAULT_EXECUTION_TIMEOUT_MS: u64 = 20_000;
pub(crate) const JINA_MAX_CONTENT_BYTES: usize = 100_000;
pub(crate) const JINA_TOKEN_BUDGET_DEFAULT: u64 = 200_000;
pub(crate) const JINA_TOKEN_BUDGET_MAX: u64 = 500_000;
pub use execute::execute_action;

pub struct JinaDomainFactory {
    execution_timeout_ms: u64,
}

impl JinaDomainFactory {
    pub fn new() -> Self {
        Self {
            execution_timeout_ms: JINA_DEFAULT_EXECUTION_TIMEOUT_MS,
        }
    }
}

impl Default for JinaDomainFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainFactory for JinaDomainFactory {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: JINA_CAPABILITY_DOMAIN_ID,
            name: "Jina Reader",
            description: "Web page reading capability domain backed by Jina Reader API. Fetches one absolute HTTP(S) URL and returns extracted markdown content plus source metadata.",
            schema_version: 1,
        }
    }

    fn actions(&self) -> Vec<CapabilityActionDefinition> {
        vec![jina_read_url::definition()]
    }

    fn create_instance(
        &self,
        _session_context: CapabilityDomainSessionContext,
    ) -> Box<dyn DomainInstance> {
        Box::new(JinaDomainInstance::new(self.execution_timeout_ms))
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Read a known page".to_string(),
                steps: vec![
                    "Call `jina__read_url` with one absolute HTTP(S) URL when you already know the page to inspect.".to_string(),
                    "Review the returned title, source URL, and extracted content before deciding whether a narrower read is needed.".to_string(),
                    "If the content is truncated or incomplete, rerun with tighter options rather than repeating the same broad request.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Target noisy page content".to_string(),
                steps: vec![
                    "Set `target_selector` when only one section of the page is relevant.".to_string(),
                    "Set `remove_selector` to exclude repeated banners or unrelated sections from the extraction.".to_string(),
                    "Set `wait_for_selector` when the relevant content appears after page load.".to_string(),
                    "Omit selector fields entirely when you do not need them.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Control extraction size and latency".to_string(),
                steps: vec![
                    "Use `token_budget` to cap how much content is returned from large pages.".to_string(),
                    "Use `timeout_ms` to constrain reads when the page is slow.".to_string(),
                    "Adjust one option at a time when tuning a request so the effect of each change is visible.".to_string(),
                ],
            },
        ]
    }
}

struct JinaDomainInstance {
    execution_timeout_ms: u64,
}

impl JinaDomainInstance {
    fn new(execution_timeout_ms: u64) -> Self {
        Self {
            execution_timeout_ms,
        }
    }
}

impl DomainInstance for JinaDomainInstance {
    fn execute_actions<'a>(
        &'a mut self,
        submissions: Vec<CapabilityActionSubmission>,
    ) -> DomainInstanceFuture<'a> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(submissions.len());
            for submission in submissions {
                results.push(execute_submission(self.execution_timeout_ms, submission).await);
            }
            results
        })
    }
}

async fn execute_submission(
    execution_timeout_ms: u64,
    submission: CapabilityActionSubmission,
) -> CapabilityActionResult {
    let Some(action_name) = action_name_for_key(submission.action_key) else {
        return CapabilityActionResult::runtime_error(
            "unknown_action_key",
            format!(
                "jina domain instance does not recognize action key {}",
                submission.action_key.0
            ),
            None,
            0,
        );
    };
    let args_json = match serde_json::to_string(&submission.args) {
        Ok(args_json) => args_json,
        Err(error) => {
            return CapabilityActionResult::runtime_error(
                "invalid_submission_args",
                format!("failed to serialize action arguments: {error}"),
                None,
                0,
            );
        }
    };

    let started_at = Instant::now();
    let mut result = execute_action(action_name, &args_json, &Value::Null, execution_timeout_ms)
        .await
        .unwrap_or_else(|| {
            CapabilityActionResult::runtime_error(
                "unknown_action",
                format!("jina action `{action_name}` is not implemented"),
                None,
                0,
            )
        });
    if result.execution_time_ms == 0 {
        result.execution_time_ms =
            started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    }
    result
}

fn action_name_for_key(key: fathom_capability_domain::CapabilityActionKey) -> Option<&'static str> {
    match key {
        jina_read_url::JINA_READ_URL_ACTION_KEY => Some("read_url"),
        _ => None,
    }
}

#[cfg(test)]
mod factory_tests {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use super::{JinaDomainFactory, jina_read_url};
    use fathom_capability_domain::{
        ActionError, CapabilityActionSubmission, CapabilityDomainSessionContext, DomainFactory,
    };
    use serde_json::json;

    #[test]
    fn jina_factory_instance_executes_action_path() {
        let mut instance =
            JinaDomainFactory::new().create_instance(CapabilityDomainSessionContext {
                session_id: "session-test".to_string(),
            });

        let results = block_on(instance.execute_actions(vec![CapabilityActionSubmission {
            action_key: jina_read_url::JINA_READ_URL_ACTION_KEY,
            args: json!({ "url": "" }),
        }]));

        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0].outcome,
            Err(ActionError::InputError(error)) if error.code == "invalid_args"
        ));
    }

    fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = noop_waker();
        let mut future = pin!(future);
        let mut context = Context::from_waker(&waker);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn noop_waker() -> Waker {
        Waker::from(Arc::new(NoopWaker))
    }

    struct NoopWaker;

    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }
}
