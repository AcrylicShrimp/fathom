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
            description: "Stateful filesystem capability domain rooted at a base path. All action paths must be non-empty relative paths under base_path; read/replace/search operate on UTF-8 text files.",
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
                title: "Locate and inspect files safely".to_string(),
                steps: vec![
                    "Use non-empty relative paths only. For root, always use path '.'. Do not use empty path, absolute path, or URI-like prefixes.".to_string(),
                    "Call filesystem__get_base_path when you need to restate current scope to the user.".to_string(),
                    "Call filesystem__list on '.' or a relative directory to discover candidates.".to_string(),
                    "Call filesystem__read on a specific file; use offset_line/limit_lines for large files.".to_string(),
                    "If read fails with invalid_encoding, report that the target is not UTF-8 text and avoid text actions on it.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Create or edit UTF-8 text files".to_string(),
                steps: vec![
                    "Confirm target path first with filesystem__list to avoid writing the wrong file.".to_string(),
                    "For full rewrites, call filesystem__write with {path, content, allow_override, create_parents?}.".to_string(),
                    "For targeted edits, call filesystem__replace with regex pattern and mode first/all.".to_string(),
                    "Use expected_replacements when correctness matters and fail loudly on mismatch.".to_string(),
                    "Read the file again after mutation to verify final content.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Search code and content".to_string(),
                steps: vec![
                    "Use filesystem__glob to discover candidate files quickly by path pattern.".to_string(),
                    "Use filesystem__search with regex to find precise references in file contents.".to_string(),
                    "Scope search using include globs and max_results to keep responses concise.".to_string(),
                    "If search hits invalid_encoding, narrow include patterns to known UTF-8 text files.".to_string(),
                ],
            },
        ]
    }
}
