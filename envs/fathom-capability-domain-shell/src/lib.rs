mod constants;
mod execute;
mod shell_run;
mod validate;

use std::sync::Arc;

use fathom_capability_domain::{
    Action, CapabilityDomain, CapabilityDomainRecipe, CapabilityDomainSpec,
};
use serde_json::{Value, json};

use shell_run::ShellRunAction;

pub const SHELL_CAPABILITY_DOMAIN_ID: &str = "shell";
pub use execute::execute_action;

pub struct ShellCapabilityDomain;

impl CapabilityDomain for ShellCapabilityDomain {
    fn spec(&self) -> CapabilityDomainSpec {
        CapabilityDomainSpec {
            id: SHELL_CAPABILITY_DOMAIN_ID,
            name: "Shell",
            description: "Workspace-scoped shell capability domain rooted at a base path. Runs non-interactive commands in base-path-relative directories with bounded output and runtime-managed timeouts.",
        }
    }

    fn initial_state(&self) -> Value {
        json!({
            "base_path": "."
        })
    }

    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![Arc::new(ShellRunAction)]
    }

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        vec![
            CapabilityDomainRecipe {
                title: "Run a bounded diagnostic command".to_string(),
                steps: vec![
                    "Call `shell__run` with one focused non-interactive command and `path: \".\"` when the domain root is the intended working directory.".to_string(),
                    "Inspect `exit_code`, `stdout`, and `stderr` in the result before deciding the next step.".to_string(),
                    "If output is truncated, rerun with a narrower command so the missing detail fits in one result.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Run work in a specific directory".to_string(),
                steps: vec![
                    "Set `path` to the non-empty relative directory where the command should run.".to_string(),
                    "Keep the command scoped to one task so failures are easy to interpret.".to_string(),
                    "If the command fails, adjust the command or working directory and rerun with a narrower goal.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Run with environment overrides".to_string(),
                steps: vec![
                    "Provide `env` only for variables the command actually depends on.".to_string(),
                    "Use valid environment keys and string values only.".to_string(),
                    "If the command times out, narrow the command, reduce output, or break the work into smaller commands.".to_string(),
                ],
            },
            CapabilityDomainRecipe {
                title: "Start longer-running shell work".to_string(),
                steps: vec![
                    "Use `shell__run` when the command may continue beyond the current turn.".to_string(),
                    "Request detached execution only when the result is not required before responding.".to_string(),
                    "Keep the command and working directory focused so later status and result updates remain interpretable.".to_string(),
                ],
            },
        ]
    }
}
