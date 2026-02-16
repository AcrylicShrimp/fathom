mod execute;
mod jina_read_url;
mod validate;

use std::sync::Arc;

use fathom_env::{Action, Environment, EnvironmentRecipe, EnvironmentSpec};
use serde_json::{Value, json};

use jina_read_url::JinaReadUrlAction;

pub const JINA_ENVIRONMENT_ID: &str = "jina";
pub(crate) const JINA_ACTION_MAX_TIMEOUT_MS: u64 = 30_000;
pub(crate) const JINA_ACTION_DESIRED_TIMEOUT_MS: u64 = 10_000;
pub(crate) const JINA_MAX_CONTENT_BYTES: usize = 100_000;
pub use execute::execute_action;

pub struct JinaEnvironment;

impl Environment for JinaEnvironment {
    fn spec(&self) -> EnvironmentSpec {
        EnvironmentSpec {
            id: JINA_ENVIRONMENT_ID,
            name: "Jina Reader",
            description: "Reader environment backed by Jina Reader API. Extracts webpage content as markdown from absolute http(s) URLs.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({})
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![Arc::new(JinaReadUrlAction)]
    }

    fn recipes(&self) -> Vec<EnvironmentRecipe> {
        vec![
            EnvironmentRecipe {
                title: "Read a specific URL".to_string(),
                steps: vec![
                    "Call jina__read_url with one absolute http(s) URL when you need readable page content.".to_string(),
                    "Prefer URLs discovered from search results or user-provided links.".to_string(),
                    "Use extracted title and source_url fields when citing facts back to the user.".to_string(),
                ],
            },
            EnvironmentRecipe {
                title: "Handle large pages safely".to_string(),
                steps: vec![
                    "Check truncated=true to detect content limits on large pages.".to_string(),
                    "When truncated, summarize with explicit partial-context caveat and request narrower follow-up targets if needed.".to_string(),
                    "Avoid chaining excessive reads in one turn when early reads already contain sufficient evidence.".to_string(),
                ],
            },
        ]
    }
}
