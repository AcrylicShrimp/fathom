use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use serde_json::{Value, json};

use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;
use crate::util::now_unix_ms;

use fathom_env::{
    Action, ActionCall, ActionOutcome, Environment, EnvironmentSnapshot, FinalizedAction,
    TransitionResult, canonical_action_id, parse_action_id,
};

use super::host::ServerActionHost;
use super::system::SystemEnvironment;

#[derive(Clone)]
pub(crate) struct ResolvedAction {
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) environment: Arc<dyn Environment>,
    pub(crate) action: Arc<dyn Action>,
}

#[derive(Clone)]
pub(crate) struct EnvironmentRegistry {
    inner: Arc<EnvironmentRegistryInner>,
}

struct EnvironmentRegistryInner {
    environments: BTreeMap<&'static str, Arc<dyn Environment>>,
    actions: BTreeMap<String, RegisteredAction>,
}

struct RegisteredAction {
    canonical_action_id: String,
    environment_id: &'static str,
    action_name: &'static str,
    environment: Arc<dyn Environment>,
    action: Arc<dyn Action>,
}

impl EnvironmentRegistry {
    pub(crate) fn new() -> Self {
        default_registry().clone()
    }

    pub(crate) fn openai_action_definitions(&self) -> Vec<Value> {
        self.inner
            .actions
            .values()
            .map(|entry| {
                let spec = entry.action.spec();
                json!({
                    "type": "function",
                    "name": entry.canonical_action_id,
                    "description": spec.description,
                    "parameters": spec.input_schema,
                })
            })
            .collect()
    }

    pub(crate) fn known_action_ids() -> Vec<String> {
        default_registry()
            .inner
            .actions
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    }

    pub(crate) fn discovery_action_ids() -> Vec<String> {
        default_registry()
            .inner
            .actions
            .values()
            .filter_map(|entry| {
                let spec = entry.action.spec();
                if spec.discovery {
                    Some(entry.canonical_action_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn default_engaged_environment_ids() -> Vec<String> {
        default_registry()
            .inner
            .environments
            .keys()
            .map(|env_id| (*env_id).to_string())
            .collect()
    }

    pub(crate) fn initial_environment_snapshots() -> BTreeMap<String, EnvironmentSnapshot> {
        let now = now_unix_ms();
        default_registry()
            .inner
            .environments
            .values()
            .map(|environment| {
                let spec = environment.spec();
                (
                    spec.id.to_string(),
                    EnvironmentSnapshot {
                        env_id: spec.id.to_string(),
                        schema_version: environment.schema_version(),
                        state_json: environment.initial_state(),
                        updated_at_unix_ms: now,
                    },
                )
            })
            .collect()
    }

    pub(crate) fn canonicalize_action_id(action_id: &str) -> Option<String> {
        let (environment_id, action_name) = parse_action_id(action_id)?;
        Some(canonical_action_id(&environment_id, &action_name))
    }

    pub(crate) fn validate(action_id: &str, args: &Value) -> Result<String, String> {
        let canonical_action_id = Self::canonicalize_action_id(action_id)
            .ok_or_else(|| format!("unknown action `{action_id}`"))?;
        let Some(entry) = default_registry().inner.actions.get(&canonical_action_id) else {
            return Err(format!("unknown action `{action_id}`"));
        };
        entry.action.validate(args)?;
        Ok(canonical_action_id)
    }

    pub(crate) fn resolve(action_id: &str) -> Option<ResolvedAction> {
        let canonical_action_id = Self::canonicalize_action_id(action_id)?;
        default_registry().resolve_by_canonical_id(&canonical_action_id)
    }

    pub(crate) fn resolve_canonical(canonical_action_id: &str) -> Option<ResolvedAction> {
        default_registry().resolve_by_canonical_id(canonical_action_id)
    }

    pub(crate) async fn execute_action(
        runtime: &Runtime,
        context: &TaskExecutionContext,
        resolved: &ResolvedAction,
        args_json: &str,
        environment_state: &Value,
    ) -> Option<ActionOutcome> {
        let args_value = serde_json::from_str::<Value>(args_json).unwrap_or(Value::Null);
        let host = ServerActionHost::new(runtime, context);
        resolved
            .action
            .execute(ActionCall {
                host: &host,
                args_json,
                args: &args_value,
                environment_state,
            })
            .await
    }

    pub(crate) fn apply_transition(
        resolved: &ResolvedAction,
        current_state: &Value,
        finalized: &FinalizedAction,
    ) -> Result<TransitionResult, String> {
        resolved
            .environment
            .apply_transition(current_state, finalized)
    }

    fn build() -> Self {
        let mut environments: BTreeMap<&'static str, Arc<dyn Environment>> = BTreeMap::new();

        register_environment(
            &mut environments,
            Arc::new(fathom_env_fs::FilesystemEnvironment),
        );
        register_environment(&mut environments, Arc::new(SystemEnvironment));

        let mut actions: BTreeMap<String, RegisteredAction> = BTreeMap::new();
        for environment in environments.values() {
            for action in environment.actions() {
                let spec = action.spec();
                let canonical_action_id =
                    canonical_action_id(spec.environment_id, spec.action_name);
                let entry = RegisteredAction {
                    canonical_action_id: canonical_action_id.clone(),
                    environment_id: spec.environment_id,
                    action_name: spec.action_name,
                    environment: environment.clone(),
                    action,
                };
                let old = actions.insert(canonical_action_id.clone(), entry);
                assert!(
                    old.is_none(),
                    "duplicate action registration for `{canonical_action_id}`"
                );
            }
        }

        Self {
            inner: Arc::new(EnvironmentRegistryInner {
                environments,
                actions,
            }),
        }
    }

    fn resolve_by_canonical_id(&self, canonical_action_id: &str) -> Option<ResolvedAction> {
        let entry = self.inner.actions.get(canonical_action_id)?;
        Some(ResolvedAction {
            canonical_action_id: entry.canonical_action_id.clone(),
            environment_id: entry.environment_id.to_string(),
            action_name: entry.action_name.to_string(),
            environment: entry.environment.clone(),
            action: entry.action.clone(),
        })
    }
}

fn default_registry() -> &'static EnvironmentRegistry {
    static REGISTRY: OnceLock<EnvironmentRegistry> = OnceLock::new();
    REGISTRY.get_or_init(EnvironmentRegistry::build)
}

fn register_environment(
    environments: &mut BTreeMap<&'static str, Arc<dyn Environment>>,
    environment: Arc<dyn Environment>,
) {
    let id = environment.spec().id;
    let old = environments.insert(id, environment);
    assert!(
        old.is_none(),
        "duplicate environment registration for `{id}`"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::EnvironmentRegistry;

    #[test]
    fn known_actions_align_with_openai_definitions() {
        let registry = EnvironmentRegistry::new();
        let names = EnvironmentRegistry::known_action_ids();
        let definitions = registry.openai_action_definitions();

        assert_eq!(definitions.len(), names.len());
    }

    #[test]
    fn validates_filesystem_path_prefix() {
        let invalid =
            EnvironmentRegistry::validate("filesystem__read", &json!({"path":"./no-scheme.txt"}));
        assert!(invalid.is_err());

        let valid =
            EnvironmentRegistry::validate("filesystem__read", &json!({"path":"fs://notes.txt"}));
        assert!(valid.is_ok());
    }
}
