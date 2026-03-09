use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::runtime::Runtime;
use crate::session::execution_context::ExecutionContext;
use crate::util::now_unix_ms;

use fathom_env::{
    Action, ActionModeSupport, ActionOutcome, Environment, EnvironmentSnapshot, FinalizedAction,
    TransitionResult, canonical_action_id, parse_action_id,
};

use super::system::SystemEnvironment;

#[derive(Debug, Clone)]
pub(crate) struct ActionTimeoutPolicy {
    pub(crate) max_timeout_ms: u64,
    pub(crate) desired_timeout_ms: Option<u64>,
}

impl ActionTimeoutPolicy {
    pub(crate) fn effective_timeout_ms(&self) -> Result<u64, String> {
        if self.max_timeout_ms == 0 {
            return Err("max_timeout_ms must be > 0".to_string());
        }

        let timeout_ms = self.desired_timeout_ms.unwrap_or(self.max_timeout_ms);
        if timeout_ms == 0 {
            return Err("desired_timeout_ms must be > 0 when set".to_string());
        }
        if timeout_ms > self.max_timeout_ms {
            return Err(format!(
                "desired_timeout_ms ({timeout_ms}) exceeds max_timeout_ms ({})",
                self.max_timeout_ms
            ));
        }

        Ok(timeout_ms)
    }
}

#[derive(Clone)]
pub(crate) struct ResolvedAction {
    pub(crate) canonical_action_id: String,
    pub(crate) environment_id: String,
    pub(crate) action_name: String,
    pub(crate) mode_support: ActionModeSupport,
    pub(crate) environment: Arc<dyn Environment>,
    pub(crate) timeout_policy: ActionTimeoutPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestedExecutionMode {
    Await,
    Detach,
}

impl RequestedExecutionMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Await => "await",
            Self::Detach => "detach",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "await" => Some(Self::Await),
            "detach" => Some(Self::Detach),
            _ => None,
        }
    }
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
    timeout_policy: ActionTimeoutPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ActivatedEnvironmentSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) recipes: Vec<fathom_env::EnvironmentRecipe>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EnvironmentActionSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) discovery: bool,
    pub(crate) mode_support: ActionModeSupport,
    pub(crate) input_schema: Value,
}

impl EnvironmentRegistry {
    pub(crate) fn new() -> Self {
        default_registry().clone()
    }

    #[cfg(test)]
    pub(crate) fn openai_action_definitions(&self) -> Vec<Value> {
        let environment_ids = self
            .inner
            .environments
            .keys()
            .map(|env_id| (*env_id).to_string())
            .collect::<BTreeSet<_>>();
        self.openai_action_definitions_for_environments(&environment_ids)
    }

    pub(crate) fn openai_action_definitions_for_environments(
        &self,
        environment_ids: &BTreeSet<String>,
    ) -> Vec<Value> {
        self.inner
            .actions
            .values()
            .filter(|entry| environment_ids.contains(entry.environment_id))
            .map(|entry| {
                let spec = entry.action.spec();
                json!({
                    "type": "function",
                    "name": entry.canonical_action_id,
                    "description": spec.description,
                    "parameters": with_reasoning_schema(spec.input_schema, spec.mode_support),
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn known_action_ids() -> Vec<String> {
        default_registry()
            .inner
            .actions
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    }

    pub(crate) fn default_engaged_environment_ids() -> Vec<String> {
        default_registry()
            .inner
            .environments
            .keys()
            .map(|env_id| (*env_id).to_string())
            .collect()
    }

    pub(crate) fn activated_environment_summaries(
        environment_ids: &[String],
    ) -> Vec<ActivatedEnvironmentSummary> {
        let registry = default_registry();
        environment_ids
            .iter()
            .filter_map(|environment_id| registry.lookup_environment_summary(environment_id))
            .collect()
    }

    pub(crate) fn environment_summary(env_id: &str) -> Option<ActivatedEnvironmentSummary> {
        default_registry().lookup_environment_summary(env_id)
    }

    pub(crate) fn environment_action_summaries(
        env_id: &str,
    ) -> Option<Vec<EnvironmentActionSummary>> {
        default_registry().lookup_environment_action_summaries(env_id)
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

    #[cfg(test)]
    pub(crate) fn validate(action_id: &str, args: &Value) -> Result<String, String> {
        let environment_ids = default_registry()
            .inner
            .environments
            .keys()
            .map(|env_id| (*env_id).to_string())
            .collect::<BTreeSet<_>>();
        default_registry().validate_in_environments(action_id, args, &environment_ids)
    }

    pub(crate) fn validate_in_environments(
        &self,
        action_id: &str,
        args: &Value,
        environment_ids: &BTreeSet<String>,
    ) -> Result<String, String> {
        let canonical_action_id = Self::canonicalize_action_id(action_id)
            .ok_or_else(|| format!("unknown action `{action_id}`"))?;
        let Some(entry) = self.inner.actions.get(&canonical_action_id) else {
            return Err(format!("unknown action `{action_id}`"));
        };
        if !environment_ids.contains(entry.environment_id) {
            return Err(format!(
                "action `{canonical_action_id}` is not available in this session"
            ));
        }
        validate_reasoning_arg(&canonical_action_id, args)?;
        validate_execution_mode_arg(&canonical_action_id, args, entry.action.spec().mode_support)?;
        entry.action.validate(args)?;
        Ok(canonical_action_id)
    }

    pub(crate) fn resolve(action_id: &str) -> Option<ResolvedAction> {
        let canonical_action_id = Self::canonicalize_action_id(action_id)?;
        default_registry().resolve_by_canonical_id(&canonical_action_id)
    }

    pub(crate) async fn execute_action(
        runtime: &Runtime,
        context: &ExecutionContext,
        resolved: &ResolvedAction,
        args_json: &str,
        environment_state: &Value,
        execution_timeout_ms: u64,
    ) -> Option<ActionOutcome> {
        let execution_args_json = strip_runtime_fields_from_args_json(args_json);
        match resolved.environment_id.as_str() {
            fathom_env_fs::FILESYSTEM_ENVIRONMENT_ID => fathom_env_fs::execute_action(
                resolved.action_name.as_str(),
                &execution_args_json,
                environment_state,
            ),
            fathom_env_brave_search::BRAVE_SEARCH_ENVIRONMENT_ID => {
                fathom_env_brave_search::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    environment_state,
                    execution_timeout_ms,
                )
                .await
            }
            fathom_env_jina::JINA_ENVIRONMENT_ID => {
                fathom_env_jina::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    environment_state,
                    execution_timeout_ms,
                )
                .await
            }
            fathom_env_shell::SHELL_ENVIRONMENT_ID => {
                fathom_env_shell::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    environment_state,
                    execution_timeout_ms,
                )
                .await
            }
            "system" => {
                crate::system_env::execute_action(
                    runtime,
                    context,
                    resolved.action_name.as_str(),
                    &execution_args_json,
                )
                .await
            }
            _ => None,
        }
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
        register_environment(
            &mut environments,
            Arc::new(fathom_env_brave_search::BraveSearchEnvironment),
        );
        register_environment(
            &mut environments,
            Arc::new(fathom_env_jina::JinaEnvironment),
        );
        register_environment(
            &mut environments,
            Arc::new(fathom_env_shell::ShellEnvironment),
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
                    timeout_policy: ActionTimeoutPolicy {
                        max_timeout_ms: spec.max_timeout_ms,
                        desired_timeout_ms: spec.desired_timeout_ms,
                    },
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
            mode_support: entry.action.spec().mode_support,
            environment: entry.environment.clone(),
            timeout_policy: entry.timeout_policy.clone(),
        })
    }

    fn lookup_environment_summary(&self, env_id: &str) -> Option<ActivatedEnvironmentSummary> {
        let environment = self.inner.environments.get(env_id)?;
        let spec = environment.spec();
        Some(ActivatedEnvironmentSummary {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            recipes: environment.recipes(),
        })
    }

    fn lookup_environment_action_summaries(
        &self,
        env_id: &str,
    ) -> Option<Vec<EnvironmentActionSummary>> {
        if !self.inner.environments.contains_key(env_id) {
            return None;
        }

        let actions = self
            .inner
            .actions
            .values()
            .filter(|entry| entry.environment_id == env_id)
            .map(|entry| {
                let spec = entry.action.spec();
                EnvironmentActionSummary {
                    id: entry.canonical_action_id.clone(),
                    name: spec.action_name.to_string(),
                    description: spec.description.to_string(),
                    discovery: spec.discovery,
                    mode_support: spec.mode_support,
                    input_schema: spec.input_schema,
                }
            })
            .collect::<Vec<_>>();

        Some(actions)
    }
}

const ACTION_REASONING_KEY: &str = "reasoning";
const ACTION_REASONING_MAX_CHARS: usize = 1_024;
const ACTION_EXECUTION_MODE_KEY: &str = "execution_mode";

fn with_reasoning_schema(input_schema: Value, mode_support: ActionModeSupport) -> Value {
    let Value::Object(mut schema) = input_schema else {
        return input_schema;
    };
    ensure_schema_object_properties(&mut schema, mode_support);
    ensure_schema_required_reasoning(&mut schema);
    Value::Object(schema)
}

fn ensure_schema_object_properties(
    schema: &mut Map<String, Value>,
    mode_support: ActionModeSupport,
) {
    let properties_entry = schema
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(properties) = properties_entry else {
        return;
    };
    properties
        .entry(ACTION_REASONING_KEY.to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": ACTION_REASONING_MAX_CHARS,
                "description": "Short reason for why this action is needed now."
            })
        });
    let supported_execution_modes = match mode_support {
        ActionModeSupport::AwaitOnly => vec![RequestedExecutionMode::Await.as_str()],
        ActionModeSupport::AwaitOrDetach => vec![
            RequestedExecutionMode::Await.as_str(),
            RequestedExecutionMode::Detach.as_str(),
        ],
    };
    properties
        .entry(ACTION_EXECUTION_MODE_KEY.to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "enum": supported_execution_modes,
                "description": "Optional execution mode override. Omit this field or use `await` unless the capability surface says the action supports detach."
            })
        });
}

fn ensure_schema_required_reasoning(schema: &mut Map<String, Value>) {
    let required_entry = schema
        .entry("required".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(required) = required_entry else {
        return;
    };
    if required
        .iter()
        .any(|value| value.as_str() == Some(ACTION_REASONING_KEY))
    {
        return;
    }
    required.push(Value::String(ACTION_REASONING_KEY.to_string()));
}

fn validate_reasoning_arg(action_id: &str, args: &Value) -> Result<(), String> {
    let Some(args) = args.as_object() else {
        return Err("action arguments must be a JSON object".to_string());
    };
    let reasoning = args
        .get(ACTION_REASONING_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "action `{action_id}` validation failed: missing non-empty string field `reasoning`"
            )
        })?;
    if reasoning.chars().count() > ACTION_REASONING_MAX_CHARS {
        return Err(format!(
            "action `{action_id}` validation failed: field `reasoning` exceeds {ACTION_REASONING_MAX_CHARS} characters"
        ));
    }
    Ok(())
}

fn validate_execution_mode_arg(
    action_id: &str,
    args: &Value,
    mode_support: ActionModeSupport,
) -> Result<(), String> {
    let Some(args) = args.as_object() else {
        return Err("action arguments must be a JSON object".to_string());
    };
    let Some(raw_mode) = args.get(ACTION_EXECUTION_MODE_KEY) else {
        return Ok(());
    };
    let raw_mode = raw_mode.as_str().ok_or_else(|| {
        format!(
            "action `{action_id}` validation failed: field `{ACTION_EXECUTION_MODE_KEY}` must be a string when set"
        )
    })?;
    let Some(mode) = RequestedExecutionMode::parse(raw_mode) else {
        return Err(format!(
            "action `{action_id}` validation failed: field `{ACTION_EXECUTION_MODE_KEY}` must be `await` or `detach`"
        ));
    };
    if mode == RequestedExecutionMode::Detach && mode_support != ActionModeSupport::AwaitOrDetach {
        return Err(format!(
            "action `{action_id}` validation failed: detach is not allowed for this action; use await"
        ));
    }
    Ok(())
}

pub(crate) fn requested_execution_mode_from_args_json(
    args_json: &str,
) -> Result<RequestedExecutionMode, String> {
    let Ok(value) = serde_json::from_str::<Value>(args_json) else {
        return Ok(RequestedExecutionMode::Await);
    };
    let Some(args) = value.as_object() else {
        return Ok(RequestedExecutionMode::Await);
    };
    let Some(raw_mode) = args.get(ACTION_EXECUTION_MODE_KEY) else {
        return Ok(RequestedExecutionMode::Await);
    };
    let Some(raw_mode) = raw_mode.as_str() else {
        return Err(format!(
            "field `{ACTION_EXECUTION_MODE_KEY}` must be a string when set"
        ));
    };
    RequestedExecutionMode::parse(raw_mode)
        .ok_or_else(|| format!("field `{ACTION_EXECUTION_MODE_KEY}` must be `await` or `detach`"))
}

fn strip_runtime_fields_from_args_json(args_json: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(args_json) else {
        return args_json.to_string();
    };
    let Some(args) = value.as_object_mut() else {
        return args_json.to_string();
    };
    args.remove(ACTION_REASONING_KEY);
    args.remove(ACTION_EXECUTION_MODE_KEY);
    serde_json::to_string(&value).unwrap_or_else(|_| args_json.to_string())
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
    use std::collections::BTreeSet;

    use fathom_env::ActionModeSupport;
    use serde_json::json;

    use super::{
        ActionTimeoutPolicy, EnvironmentRegistry, RequestedExecutionMode,
        requested_execution_mode_from_args_json, validate_execution_mode_arg,
    };

    #[test]
    fn known_actions_align_with_openai_definitions() {
        let registry = EnvironmentRegistry::new();
        let names = EnvironmentRegistry::known_action_ids();
        let definitions = registry.openai_action_definitions();

        assert_eq!(definitions.len(), names.len());
    }

    #[test]
    fn validates_filesystem_relative_path() {
        let invalid =
            EnvironmentRegistry::validate("filesystem__read", &json!({"path":"fs://notes.txt"}));
        assert!(invalid.is_err());

        let valid = EnvironmentRegistry::validate(
            "filesystem__read",
            &json!({"path":"notes.txt", "reasoning":"read file requested by user"}),
        );
        assert!(valid.is_ok());
    }

    #[test]
    fn validates_reasoning_requirement() {
        let missing_reasoning =
            EnvironmentRegistry::validate("filesystem__read", &json!({"path":"notes.txt"}));
        assert!(missing_reasoning.is_err());

        let empty_reasoning = EnvironmentRegistry::validate(
            "filesystem__read",
            &json!({"path":"notes.txt","reasoning":"   "}),
        );
        assert!(empty_reasoning.is_err());
    }

    #[test]
    fn activated_environment_summaries_include_name_and_description() {
        let summaries = EnvironmentRegistry::activated_environment_summaries(&[
            "filesystem".to_string(),
            "system".to_string(),
        ]);
        assert!(
            summaries
                .iter()
                .any(|summary| summary.id == "filesystem" && !summary.name.is_empty())
        );
        assert!(
            summaries
                .iter()
                .any(|summary| summary.id == "system" && !summary.description.is_empty())
        );
    }

    #[test]
    fn filesystem_list_definition_includes_root_path_guidance() {
        let registry = EnvironmentRegistry::new();
        let definitions = registry.openai_action_definitions();

        let list_definition = definitions
            .iter()
            .find(|definition| definition["name"] == json!("filesystem__list"))
            .expect("filesystem__list definition should exist");
        let description = list_definition["description"]
            .as_str()
            .expect("description should be a string");

        assert!(description.contains("non-empty relative"));
        assert!(description.contains("use `.`"));
    }

    #[test]
    fn openai_definitions_require_reasoning_field() {
        let registry = EnvironmentRegistry::new();
        let definitions = registry.openai_action_definitions();

        for definition in definitions {
            let required = definition["parameters"]["required"]
                .as_array()
                .expect("required must be an array");
            assert!(
                required
                    .iter()
                    .any(|entry| entry.as_str() == Some("reasoning")),
                "reasoning must be required for {}",
                definition["name"]
            );
        }
    }

    #[test]
    fn shell_run_definition_exists() {
        let registry = EnvironmentRegistry::new();
        let definitions = registry.openai_action_definitions();

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("shell__run"))
        );
    }

    #[test]
    fn brave_search_web_search_definition_exists() {
        let registry = EnvironmentRegistry::new();
        let definitions = registry.openai_action_definitions();

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("brave_search__web_search"))
        );
    }

    #[test]
    fn jina_read_url_definition_exists() {
        let registry = EnvironmentRegistry::new();
        let definitions = registry.openai_action_definitions();

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("jina__read_url"))
        );
    }

    #[test]
    fn filtered_openai_definitions_only_include_allowed_environments() {
        let registry = EnvironmentRegistry::new();
        let definitions = registry.openai_action_definitions_for_environments(&BTreeSet::from([
            "filesystem".to_string(),
            "system".to_string(),
        ]));

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("filesystem__read"))
        );
        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("system__get_context"))
        );
        assert!(
            definitions
                .iter()
                .all(|definition| definition["name"] != json!("shell__run"))
        );
    }

    #[test]
    fn validate_in_environments_rejects_action_outside_allowed_session_set() {
        let registry = EnvironmentRegistry::new();
        let allowed = BTreeSet::from(["filesystem".to_string()]);

        let error = registry
            .validate_in_environments(
                "shell__run",
                &json!({"command":"pwd","reasoning":"inspect cwd"}),
                &allowed,
            )
            .expect_err("shell action should be rejected when shell is not engaged");
        assert!(error.contains("is not available in this session"));
    }

    #[test]
    fn registered_actions_have_valid_timeout_policies() {
        let action_ids = EnvironmentRegistry::known_action_ids();
        for action_id in action_ids {
            let resolved =
                EnvironmentRegistry::resolve(&action_id).expect("registered action should resolve");
            assert!(
                resolved.timeout_policy.effective_timeout_ms().is_ok(),
                "timeout policy for `{action_id}` must be valid"
            );
        }
    }

    #[test]
    fn timeout_policy_rejects_desired_over_max() {
        let policy = ActionTimeoutPolicy {
            max_timeout_ms: 1_000,
            desired_timeout_ms: Some(1_001),
        };
        assert!(policy.effective_timeout_ms().is_err());
    }

    #[test]
    fn strips_runtime_reasoning_field_for_execution() {
        let raw = r#"{"query":"seti","count":5,"reasoning":"need sources"}"#;
        let stripped = super::strip_runtime_fields_from_args_json(raw);
        let value: serde_json::Value =
            serde_json::from_str(&stripped).expect("stripped args must be valid json");
        assert!(value.get("reasoning").is_none());
        assert_eq!(value["query"], "seti");
        assert_eq!(value["count"], 5);
    }

    #[test]
    fn strips_execution_mode_field_for_execution() {
        let raw =
            r#"{"query":"seti","count":5,"reasoning":"need sources","execution_mode":"detach"}"#;
        let stripped = super::strip_runtime_fields_from_args_json(raw);
        let value: serde_json::Value =
            serde_json::from_str(&stripped).expect("stripped args must be valid json");
        assert!(value.get("reasoning").is_none());
        assert!(value.get("execution_mode").is_none());
        assert_eq!(value["query"], "seti");
        assert_eq!(value["count"], 5);
    }

    #[test]
    fn requested_execution_mode_defaults_to_await() {
        let mode = requested_execution_mode_from_args_json(
            r#"{"query":"seti","reasoning":"need sources"}"#,
        )
        .expect("mode should parse");
        assert_eq!(mode, RequestedExecutionMode::Await);
    }

    #[test]
    fn requested_execution_mode_accepts_detach() {
        let mode = requested_execution_mode_from_args_json(
            r#"{"query":"seti","reasoning":"need sources","execution_mode":"detach"}"#,
        )
        .expect("mode should parse");
        assert_eq!(mode, RequestedExecutionMode::Detach);
    }

    #[test]
    fn requested_execution_mode_rejects_invalid_values() {
        let error = requested_execution_mode_from_args_json(
            r#"{"query":"seti","reasoning":"need sources","execution_mode":"later"}"#,
        )
        .expect_err("invalid mode must be rejected");
        assert!(error.contains("must be `await` or `detach`"));
    }

    #[test]
    fn validate_execution_mode_allows_omitted_mode() {
        let result = validate_execution_mode_arg(
            "filesystem__list",
            &json!({"path":".","reasoning":"inspect workspace"}),
            ActionModeSupport::AwaitOnly,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_execution_mode_rejects_detach_for_await_only_action() {
        let error = validate_execution_mode_arg(
            "filesystem__list",
            &json!({"path":".","reasoning":"inspect workspace","execution_mode":"detach"}),
            ActionModeSupport::AwaitOnly,
        )
        .expect_err("detach must be rejected for await_only actions");
        assert!(error.contains("detach is not allowed"));
    }

    #[test]
    fn validate_execution_mode_accepts_detach_for_detach_capable_action() {
        let result = validate_execution_mode_arg(
            "system__get_context",
            &json!({"reasoning":"inspect runtime","execution_mode":"detach"}),
            ActionModeSupport::AwaitOrDetach,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn openai_definition_limits_execution_mode_enum_for_await_only_action() {
        let registry = EnvironmentRegistry::new();
        let definition = registry
            .openai_action_definitions()
            .into_iter()
            .find(|definition| definition["name"] == json!("filesystem__read"))
            .expect("filesystem__read definition should exist");

        assert_eq!(
            definition["parameters"]["properties"]["execution_mode"]["enum"],
            json!(["await"])
        );
    }

    #[test]
    fn openai_definition_allows_detach_for_detach_capable_action() {
        let registry = EnvironmentRegistry::new();
        let definition = registry
            .openai_action_definitions()
            .into_iter()
            .find(|definition| definition["name"] == json!("shell__run"))
            .expect("shell__run definition should exist");

        assert_eq!(
            definition["parameters"]["properties"]["execution_mode"]["enum"],
            json!(["await", "detach"])
        );
    }

    #[test]
    fn validate_reasoning_counts_unicode_characters_not_bytes() {
        let valid = EnvironmentRegistry::validate(
            "filesystem__read",
            &json!({"path":"notes.txt", "reasoning": "가".repeat(1024)}),
        );
        assert!(valid.is_ok());

        let invalid = EnvironmentRegistry::validate(
            "filesystem__read",
            &json!({"path":"notes.txt", "reasoning": "가".repeat(1025)}),
        )
        .expect_err("unicode reasoning beyond max should fail");
        assert!(invalid.contains("exceeds 1024 characters"));
    }
}
