mod execute;
mod jina_read_url;
mod validate;

use std::sync::Arc;

use fathom_capability_domain::{
    Action, CapabilityDomain, CapabilityDomainRecipe, CapabilityDomainSpec,
};
use serde_json::{Value, json};

use jina_read_url::JinaReadUrlAction;

pub const JINA_CAPABILITY_DOMAIN_ID: &str = "jina";
pub(crate) const JINA_ACTION_MAX_TIMEOUT_MS: u64 = 30_000;
pub(crate) const JINA_ACTION_DESIRED_TIMEOUT_MS: u64 = 30_000;
pub(crate) const JINA_MAX_CONTENT_BYTES: usize = 100_000;
pub(crate) const JINA_TOKEN_BUDGET_DEFAULT: u64 = 200_000;
pub(crate) const JINA_TOKEN_BUDGET_MAX: u64 = 500_000;
pub use execute::execute_action;

pub struct JinaCapabilityDomain;

impl CapabilityDomain for JinaCapabilityDomain {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: JINA_CAPABILITY_DOMAIN_ID,
            name: "Jina Reader",
            description: "Web page reading capability domain backed by Jina Reader API. Fetches one absolute HTTP(S) URL and returns extracted markdown content plus source metadata.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({})
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![Arc::new(JinaReadUrlAction)]
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
