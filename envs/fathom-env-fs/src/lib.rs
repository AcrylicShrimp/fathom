mod fs_list;
mod fs_read;
mod fs_replace;
mod fs_write;
mod validate;

use std::sync::Arc;

use fathom_env::{Action, Environment, EnvironmentSpec};
use serde_json::{Value, json};

use fs_list::FsListAction;
use fs_read::FsReadAction;
use fs_replace::FsReplaceAction;
use fs_write::FsWriteAction;

pub const FILESYSTEM_ENVIRONMENT_ID: &str = "filesystem";

pub struct FilesystemEnvironment;

impl Environment for FilesystemEnvironment {
    fn spec(&self) -> EnvironmentSpec {
        EnvironmentSpec {
            id: FILESYSTEM_ENVIRONMENT_ID,
            description: "Stateful filesystem environment for managed:// and fs:// URI operations.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({
            "cwd_uri": "fs://"
        })
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![
            Arc::new(FsListAction),
            Arc::new(FsReadAction),
            Arc::new(FsWriteAction),
            Arc::new(FsReplaceAction),
        ]
    }
}
