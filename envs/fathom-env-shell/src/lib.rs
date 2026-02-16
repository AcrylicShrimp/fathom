mod constants;
mod execute;
mod shell_run;
mod validate;

use std::sync::Arc;

use fathom_env::{Action, Environment, EnvironmentRecipe, EnvironmentSpec};
use serde_json::{Value, json};

use shell_run::ShellRunAction;

pub const SHELL_ENVIRONMENT_ID: &str = "shell";
pub use execute::execute_action;

pub struct ShellEnvironment;

impl Environment for ShellEnvironment {
    fn spec(&self) -> EnvironmentSpec {
        EnvironmentSpec {
            id: SHELL_ENVIRONMENT_ID,
            name: "Shell",
            description: "Stateful shell command environment rooted at a base path. Execute non-interactive commands with bounded stdout/stderr and runtime-managed timeouts.",
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

    fn recipes(&self) -> Vec<EnvironmentRecipe> {
        vec![
            EnvironmentRecipe {
                title: "Run quick diagnostics".to_string(),
                steps: vec![
                    "Use shell__run with a non-interactive command and path '.' for environment root.".to_string(),
                    "Execution timeout is managed by the runtime using action policy; do not expect timeout args.".to_string(),
                    "Interpret output via exit_code, stdout, and stderr; non-zero exit means task failure.".to_string(),
                ],
            },
            EnvironmentRecipe {
                title: "Run command in a subdirectory".to_string(),
                steps: vec![
                    "Pass path as a non-empty relative directory under shell base_path.".to_string(),
                    "Confirm directory existence first (for example via filesystem__list) before running commands.".to_string(),
                    "If command fails, inspect stderr and rerun with corrected args instead of chaining risky commands.".to_string(),
                ],
            },
            EnvironmentRecipe {
                title: "Control runtime environment".to_string(),
                steps: vec![
                    "Call shell__run with {command, env} when command behavior depends on env vars.".to_string(),
                    "Use env keys matching [A-Za-z_][A-Za-z0-9_]* and pass only required variables.".to_string(),
                    "On timeout, split work into smaller commands or use narrower command scope.".to_string(),
                    "If stdout/stderr is truncated, rerun with narrower command scope to recover missing detail.".to_string(),
                ],
            },
        ]
    }
}
