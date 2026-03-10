mod execute;
mod fs_get_base_path;
mod fs_glob;
mod fs_list;
mod fs_read;
mod fs_replace;
mod fs_search;
mod fs_write;
mod validate;

use std::sync::Arc;

use fathom_capability_domain::{
    Action, CapabilityDomain, CapabilityDomainRecipe, CapabilityDomainSpec,
};
use serde_json::{Value, json};

use fs_get_base_path::FsGetBasePathAction;
use fs_glob::FsGlobAction;
use fs_list::FsListAction;
use fs_read::FsReadAction;
use fs_replace::FsReplaceAction;
use fs_search::FsSearchAction;
use fs_write::FsWriteAction;

pub const FILESYSTEM_CAPABILITY_DOMAIN_ID: &str = "filesystem";
pub(crate) const FILESYSTEM_ACTION_MAX_TIMEOUT_MS: u64 = 20_000;
pub(crate) const FILESYSTEM_ACTION_DESIRED_TIMEOUT_MS: u64 = 8_000;
pub use execute::execute_action;

pub struct FilesystemCapabilityDomain;

impl CapabilityDomain for FilesystemCapabilityDomain {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: FILESYSTEM_CAPABILITY_DOMAIN_ID,
            name: "Filesystem",
            description: "Workspace-scoped filesystem capability domain rooted at a base path. Operates on non-empty relative paths under `base_path`; `read`, `replace`, and `search` work on UTF-8 text content.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({
            "base_path": "."
        })
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(FsGetBasePathAction),
            Arc::new(FsListAction),
            Arc::new(FsReadAction),
            Arc::new(FsWriteAction),
            Arc::new(FsReplaceAction),
            Arc::new(FsGlobAction),
            Arc::new(FsSearchAction),
        ]
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Inspect files and directories".to_string(),
                steps: vec![
                    "Use `filesystem__get_base_path` when you need to inspect the current filesystem root for this domain.".to_string(),
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
