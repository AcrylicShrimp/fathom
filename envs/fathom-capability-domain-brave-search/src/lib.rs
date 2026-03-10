mod brave_web_search;
mod execute;
mod validate;

use std::sync::Arc;

use fathom_capability_domain::{
    Action, CapabilityDomain, CapabilityDomainRecipe, CapabilityDomainSpec,
};
use serde_json::{Value, json};

use brave_web_search::BraveWebSearchAction;

pub const BRAVE_SEARCH_CAPABILITY_DOMAIN_ID: &str = "brave_search";
pub(crate) const BRAVE_SEARCH_ACTION_MAX_TIMEOUT_MS: u64 = 30_000;
pub(crate) const BRAVE_SEARCH_ACTION_DESIRED_TIMEOUT_MS: u64 = 10_000;
pub(crate) const BRAVE_SEARCH_DEFAULT_COUNT: u8 = 5;
pub(crate) const BRAVE_SEARCH_MAX_COUNT: u8 = 20;
pub(crate) const BRAVE_SEARCH_DEFAULT_SAFESEARCH: &str = "off";
pub use execute::execute_action;

pub struct BraveSearchCapabilityDomain;

impl CapabilityDomain for BraveSearchCapabilityDomain {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: BRAVE_SEARCH_CAPABILITY_DOMAIN_ID,
            name: "Brave Search",
            description: "Web search capability domain backed by Brave Search API. Runs focused public-web queries and returns compact ranked result metadata such as title, URL, and description.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({})
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![Arc::new(BraveWebSearchAction)]
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
