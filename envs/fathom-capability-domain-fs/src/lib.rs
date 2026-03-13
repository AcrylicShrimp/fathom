mod execute;
mod fs_get_base_path;
mod fs_glob;
mod fs_list;
mod fs_read;
mod fs_replace;
mod fs_search;
mod fs_write;

use std::path::PathBuf;
use std::time::Instant;

use fathom_capability_domain::{
    CapabilityActionDefinition, CapabilityActionResult, CapabilityActionSubmission,
    CapabilityDomainRecipe, CapabilityDomainSessionContext, CapabilityDomainSpec, DomainFactory,
    DomainInstance, DomainInstanceFuture,
};
use serde_json::{Value, json};

pub const FILESYSTEM_CAPABILITY_DOMAIN_ID: &str = "filesystem";
pub use execute::execute_action;

pub struct FilesystemDomainFactory {
    base_path: PathBuf,
}

impl FilesystemDomainFactory {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

impl DomainFactory for FilesystemDomainFactory {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: FILESYSTEM_CAPABILITY_DOMAIN_ID,
            name: "Filesystem",
            description: "Workspace-scoped filesystem capability domain rooted at a base path. Operates on non-empty relative paths under `base_path`; `read`, `replace`, and `search` work on UTF-8 text content.",
            schema_version: 1,
        }
    }

    fn actions(&self) -> Vec<CapabilityActionDefinition> {
        vec![
            fs_get_base_path::definition(),
            fs_list::definition(),
            fs_read::definition(),
            fs_write::definition(),
            fs_replace::definition(),
            fs_glob::definition(),
            fs_search::definition(),
        ]
    }

    fn create_instance(
        &self,
        _session_context: CapabilityDomainSessionContext,
    ) -> Box<dyn DomainInstance> {
        Box::new(FilesystemDomainInstance::new(self.base_path.clone()))
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Inspect files and directories".to_string(),
                steps: vec![
                    "Use `filesystem__get_base_path` when you need to inspect the current filesystem root for this domain.".to_string(),
                    "Do not use empty path values; use path '.' to target the root directory.".to_string(),
                    "Use `filesystem__list` with `path: \".\"` or a relative directory to discover entries under the current base path.".to_string(),
                    "Use `filesystem__read` on a specific relative file path once you know the target.".to_string(),
                    "For large files, set `offset_line` and `limit_lines` to inspect only the relevant window.".to_string(),
                    "If a text action returns `invalid_encoding`, treat the target as non-UTF-8 content and stop using text-only actions on it.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Apply a targeted text change".to_string(),
                steps: vec![
                    "Use `filesystem__read` first to confirm the exact existing text at the target path.".to_string(),
                    "Call `filesystem__replace` with literal `old` and `new` strings and `mode` set to `first` or `all`.".to_string(),
                    "Set `expected_replacements` when the change must match an exact replacement count.".to_string(),
                    "Use `filesystem__read` again after the edit to verify the final content.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Find paths and content matches".to_string(),
                steps: vec![
                    "Use `filesystem__glob` when you know the path pattern but not the exact file name.".to_string(),
                    "Use `filesystem__search` when you need regex matches inside UTF-8 file contents.".to_string(),
                    "Constrain `path`, `include`, and result limits to keep the search focused.".to_string(),
                    "Refine the pattern and rerun when the initial search is too broad or too narrow.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Create or rewrite a text file".to_string(),
                steps: vec![
                    "Choose a non-empty relative file path under the current base path.".to_string(),
                    "Call `filesystem__write` with `content` and `allow_override` set for the intended create or overwrite behavior.".to_string(),
                    "Set `create_parents` when parent directories may need to be created.".to_string(),
                    "Use `filesystem__read` after writing when the final content must be verified.".to_string(),
                ],
            },
        ]
    }
}

struct FilesystemDomainInstance {
    state: Value,
}

impl FilesystemDomainInstance {
    fn new(base_path: PathBuf) -> Self {
        Self {
            state: json!({
                "base_path": base_path.to_string_lossy().to_string(),
            }),
        }
    }
}

impl DomainInstance for FilesystemDomainInstance {
    fn execute_actions<'a>(
        &'a mut self,
        submissions: Vec<CapabilityActionSubmission>,
    ) -> DomainInstanceFuture<'a> {
        Box::pin(async move {
            submissions
                .into_iter()
                .map(|submission| execute_submission(&self.state, submission))
                .collect()
        })
    }
}

fn execute_submission(
    state: &Value,
    submission: CapabilityActionSubmission,
) -> CapabilityActionResult {
    let Some(action_name) = action_name_for_key(submission.action_key) else {
        return CapabilityActionResult::runtime_error(
            "unknown_action_key",
            format!(
                "filesystem domain instance does not recognize action key {}",
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
    let mut result = execute_action(action_name, &args_json, state).unwrap_or_else(|| {
        CapabilityActionResult::runtime_error(
            "unknown_action",
            format!("filesystem action `{action_name}` is not implemented"),
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
        fs_get_base_path::FS_GET_BASE_PATH_ACTION_KEY => Some("get_base_path"),
        fs_list::FS_LIST_ACTION_KEY => Some("list"),
        fs_read::FS_READ_ACTION_KEY => Some("read"),
        fs_write::FS_WRITE_ACTION_KEY => Some("write"),
        fs_replace::FS_REPLACE_ACTION_KEY => Some("replace"),
        fs_glob::FS_GLOB_ACTION_KEY => Some("glob"),
        fs_search::FS_SEARCH_ACTION_KEY => Some("search"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use super::{FilesystemDomainFactory, fs_list};
    use fathom_capability_domain::{
        CapabilityActionSubmission, CapabilityDomainSessionContext, DomainFactory,
    };
    use serde_json::json;

    #[test]
    fn filesystem_factory_instance_executes_list_action() {
        let mut instance = FilesystemDomainFactory::new(
            std::env::current_dir().expect("current directory for filesystem factory"),
        )
        .create_instance(CapabilityDomainSessionContext {
            session_id: "session-test".to_string(),
        });

        let results = block_on(instance.execute_actions(vec![CapabilityActionSubmission {
            action_key: fs_list::FS_LIST_ACTION_KEY,
            args: json!({ "path": "." }),
        }]));

        assert_eq!(results.len(), 1);
        assert!(results[0].outcome.is_ok());
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
