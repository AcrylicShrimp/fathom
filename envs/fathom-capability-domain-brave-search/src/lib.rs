mod brave_web_search;
mod execute;

use std::time::Instant;

use fathom_capability_domain::{
    CapabilityActionDefinition, CapabilityActionResult, CapabilityActionSubmission,
    CapabilityDomainRecipe, CapabilityDomainSessionContext, CapabilityDomainSpec, DomainFactory,
    DomainInstance, DomainInstanceFuture,
};
use serde_json::Value;

pub const BRAVE_SEARCH_CAPABILITY_DOMAIN_ID: &str = "brave_search";
pub(crate) const BRAVE_SEARCH_DEFAULT_COUNT: u8 = 5;
pub(crate) const BRAVE_SEARCH_MAX_COUNT: u8 = 20;
pub(crate) const BRAVE_SEARCH_DEFAULT_SAFESEARCH: &str = "off";
pub(crate) const BRAVE_SEARCH_DEFAULT_TIMEOUT_MS: u64 = 20_000;
pub use execute::execute_action;

pub struct BraveSearchDomainFactory {
    execution_timeout_ms: u64,
}

impl BraveSearchDomainFactory {
    pub fn new() -> Self {
        Self {
            execution_timeout_ms: BRAVE_SEARCH_DEFAULT_TIMEOUT_MS,
        }
    }
}

impl Default for BraveSearchDomainFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainFactory for BraveSearchDomainFactory {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: BRAVE_SEARCH_CAPABILITY_DOMAIN_ID,
            name: "Brave Search",
            description: "Web search capability domain backed by Brave Search API. Runs focused public-web queries and returns compact ranked result metadata such as title, URL, and description.",
            schema_version: 1,
        }
    }

    fn actions(&self) -> Vec<CapabilityActionDefinition> {
        vec![brave_web_search::definition()]
    }

    fn create_instance(
        &self,
        _session_context: CapabilityDomainSessionContext,
    ) -> Box<dyn DomainInstance> {
        Box::new(BraveSearchDomainInstance::new(self.execution_timeout_ms))
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Run a focused web query".to_string(),
                steps: vec![
                    "Start with a specific query that includes the key entities or terms you need.".to_string(),
                    "Use a small `count` first to keep the result set focused.".to_string(),
                    "Inspect the ranked titles, URLs, and descriptions before deciding whether to refine the query.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Refine weak search results".to_string(),
                steps: vec![
                    "Rewrite the query with clearer names, exact phrases, dates, or constraints when the first result set is noisy.".to_string(),
                    "Increase `count` only when the initial result set does not provide enough candidate sources.".to_string(),
                    "Repeat with a narrower query when the result set is broad or off-topic.".to_string(),
                ],
            },
        ]
    }
}

struct BraveSearchDomainInstance {
    execution_timeout_ms: u64,
}

impl BraveSearchDomainInstance {
    fn new(execution_timeout_ms: u64) -> Self {
        Self {
            execution_timeout_ms,
        }
    }
}

impl DomainInstance for BraveSearchDomainInstance {
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
                "brave_search domain instance does not recognize action key {}",
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
                format!("brave_search action `{action_name}` is not implemented"),
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
        brave_web_search::BRAVE_WEB_SEARCH_ACTION_KEY => Some("web_search"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use super::{BraveSearchDomainFactory, brave_web_search};
    use fathom_capability_domain::{
        ActionError, CapabilityActionSubmission, CapabilityDomainSessionContext, DomainFactory,
    };
    use serde_json::json;

    #[test]
    fn brave_search_factory_instance_executes_action_path() {
        let mut instance =
            BraveSearchDomainFactory::new().create_instance(CapabilityDomainSessionContext {
                session_id: "session-test".to_string(),
            });

        let results = block_on(instance.execute_actions(vec![CapabilityActionSubmission {
            action_key: brave_web_search::BRAVE_WEB_SEARCH_ACTION_KEY,
            args: json!({ "query": "" }),
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
