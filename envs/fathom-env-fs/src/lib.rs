mod execute;
mod fs_get_base_path;
mod fs_list;
mod fs_read;
mod fs_replace;
mod fs_write;
mod validate;

use std::sync::Arc;

use fathom_env::{Action, Environment, EnvironmentSpec};
use serde_json::{Value, json};

use fs_get_base_path::FsGetBasePathAction;
use fs_list::FsListAction;
use fs_read::FsReadAction;
use fs_replace::FsReplaceAction;
use fs_write::FsWriteAction;

pub const FILESYSTEM_ENVIRONMENT_ID: &str = "filesystem";
pub use execute::execute_action;

pub struct FilesystemEnvironment;

impl Environment for FilesystemEnvironment {
    fn spec(&self) -> EnvironmentSpec {
        EnvironmentSpec {
            id: FILESYSTEM_ENVIRONMENT_ID,
            name: "Filesystem",
            description: "Stateful filesystem environment rooted at a base path.",
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
        ]
    }
}
