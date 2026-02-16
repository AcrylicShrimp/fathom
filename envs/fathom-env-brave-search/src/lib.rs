mod brave_web_search;
mod execute;
mod validate;

use std::sync::Arc;

use fathom_env::{Action, Environment, EnvironmentRecipe, EnvironmentSpec};
use serde_json::{Value, json};

use brave_web_search::BraveWebSearchAction;

pub const BRAVE_SEARCH_ENVIRONMENT_ID: &str = "brave_search";
pub(crate) const BRAVE_SEARCH_ACTION_MAX_TIMEOUT_MS: u64 = 30_000;
pub(crate) const BRAVE_SEARCH_ACTION_DESIRED_TIMEOUT_MS: u64 = 10_000;
pub(crate) const BRAVE_SEARCH_DEFAULT_COUNT: u8 = 5;
pub(crate) const BRAVE_SEARCH_MAX_COUNT: u8 = 20;
pub(crate) const BRAVE_SEARCH_DEFAULT_SAFESEARCH: &str = "off";
pub use execute::execute_action;

pub struct BraveSearchEnvironment;

impl Environment for BraveSearchEnvironment {
    fn spec(&self) -> EnvironmentSpec {
        EnvironmentSpec {
            id: BRAVE_SEARCH_ENVIRONMENT_ID,
            name: "Brave Search",
            description: "Web search environment backed by Brave Search API. Returns compact ranked metadata (title/url/description) for external sources.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({})
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![Arc::new(BraveWebSearchAction)]
    }

    fn recipes(&self) -> Vec<EnvironmentRecipe> {
        vec![
            EnvironmentRecipe {
                title: "Discover relevant web sources".to_string(),
                steps: vec![
                    "Call brave_search__web_search with a focused factual query and an intentional count.".to_string(),
                    "Inspect ranked title/url/description results and pick trustworthy sources to cite.".to_string(),
                    "If results are weak, refine the query with clearer entities, timeframe, or constraints.".to_string(),
                ],
            },
            EnvironmentRecipe {
                title: "Keep search outputs concise".to_string(),
                steps: vec![
                    "Start with small count values to reduce noisy context and token usage.".to_string(),
                    "Increase count only when first-pass coverage is insufficient.".to_string(),
                    "Reuse concrete source URLs from results when responding to the user.".to_string(),
                ],
            },
        ]
    }
}
