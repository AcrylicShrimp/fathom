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
pub(crate) const JINA_TOKEN_BUDGET_DEFAULT: u64 = 200_000;
pub(crate) const JINA_TOKEN_BUDGET_MAX: u64 = 500_000;
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
                    "The environment tries hard selector filtering first, then soft no-selector fallback on provider/transport failures.".to_string(),
                    "Use extracted title and source_url fields when citing facts back to the user.".to_string(),
                ],
            },
            EnvironmentRecipe {
                title: "Handle noisy pages".to_string(),
                steps: vec![
                    "Inspect advisory and attempts metadata in each result; content may still be low quality even when request succeeds.".to_string(),
                    "Optional string fields must be omitted when unused; do not pass empty strings for selector/cookie fields.".to_string(),
                    "When needed, provide advanced options (target_selector/remove_selector/wait_for_selector/set_cookie/no_cache/token_budget/timeout_ms).".to_string(),
                    "Prefer custom targeted retry before chaining many payload lookups.".to_string(),
                ],
            },
        ]
    }
}
