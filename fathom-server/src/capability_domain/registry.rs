use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::runtime::Runtime;
use crate::session::execution_context::ExecutionContext;
use crate::util::now_unix_ms;

use fathom_capability_domain::{
    Action, ActionModeSupport, ActionOutcome, CapabilityDomain, CapabilityDomainSnapshot,
    FinalizedAction, TransitionResult, canonical_action_id, parse_action_id,
};

use super::system::SystemCapabilityDomain;

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
    pub(crate) capability_domain_id: String,
    pub(crate) action_name: String,
    pub(crate) mode_support: ActionModeSupport,
    pub(crate) environment: Arc<dyn CapabilityDomain>,
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
pub(crate) struct CapabilityDomainRegistry {
    inner: Arc<CapabilityDomainRegistryInner>,
}

struct CapabilityDomainRegistryInner {
    capability_domains: BTreeMap<&'static str, Arc<dyn CapabilityDomain>>,
    actions: BTreeMap<String, RegisteredAction>,
}

struct RegisteredAction {
    canonical_action_id: String,
    capability_domain_id: &'static str,
    action_name: &'static str,
    environment: Arc<dyn CapabilityDomain>,
    action: Arc<dyn Action>,
    timeout_policy: ActionTimeoutPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ActivatedCapabilityDomainSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) recipes: Vec<fathom_capability_domain::CapabilityDomainRecipe>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityDomainActionSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) discovery: bool,
    pub(crate) mode_support: ActionModeSupport,
    pub(crate) input_schema: Value,
}

impl CapabilityDomainRegistry {
    pub(crate) fn new() -> Self {
        default_registry().clone()
    }

    #[cfg(test)]
    pub(crate) fn openai_action_definitions(&self) -> Vec<Value> {
        let capability_domain_ids = self
            .inner
            .capability_domains
            .keys()
            .map(|capability_domain_id| (*capability_domain_id).to_string())
            .collect::<BTreeSet<_>>();
        self.openai_action_definitions_for_capability_domains(&capability_domain_ids)
    }

    pub(crate) fn openai_action_definitions_for_capability_domains(
        &self,
        capability_domain_ids: &BTreeSet<String>,
    ) -> Vec<Value> {
        self.inner
            .actions
            .values()
            .filter(|entry| capability_domain_ids.contains(entry.capability_domain_id))
            .map(|entry| {
                let spec = entry.action.spec();
                json!({
                    "type": "function",
                    "name": entry.canonical_action_id,
                    "description": spec.description,
                    "parameters": with_runtime_action_schema(spec.input_schema, spec.mode_support),
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

    pub(crate) fn default_engaged_capability_domain_ids() -> Vec<String> {
        default_registry()
            .inner
            .capability_domains
            .keys()
            .map(|capability_domain_id| (*capability_domain_id).to_string())
            .collect()
    }

    pub(crate) fn activated_capability_domain_summaries(
        capability_domain_ids: &[String],
    ) -> Vec<ActivatedCapabilityDomainSummary> {
        let registry = default_registry();
        capability_domain_ids
            .iter()
            .filter_map(|capability_domain_id| {
                registry.lookup_capability_domain_summary(capability_domain_id)
            })
            .collect()
    }

    pub(crate) fn capability_domain_summary(
        capability_domain_id: &str,
    ) -> Option<ActivatedCapabilityDomainSummary> {
        default_registry().lookup_capability_domain_summary(capability_domain_id)
    }

    pub(crate) fn capability_domain_action_summaries(
        capability_domain_id: &str,
    ) -> Option<Vec<CapabilityDomainActionSummary>> {
        default_registry().lookup_capability_domain_action_summaries(capability_domain_id)
    }

    pub(crate) fn initial_capability_domain_snapshots() -> BTreeMap<String, CapabilityDomainSnapshot>
    {
        let now = now_unix_ms();
        default_registry()
            .inner
            .capability_domains
            .values()
            .map(|environment| {
                let spec = environment.spec();
                (
                    spec.id.to_string(),
                    CapabilityDomainSnapshot {
                        capability_domain_id: spec.id.to_string(),
                        schema_version: environment.schema_version(),
                        state_json: environment.initial_state(),
                        updated_at_unix_ms: now,
                    },
                )
            })
            .collect()
    }

    pub(crate) fn canonicalize_action_id(action_id: &str) -> Option<String> {
        let (capability_domain_id, action_name) = parse_action_id(action_id)?;
        Some(canonical_action_id(&capability_domain_id, &action_name))
    }

    #[cfg(test)]
    pub(crate) fn validate(action_id: &str, args: &Value) -> Result<String, String> {
        let capability_domain_ids = default_registry()
            .inner
            .capability_domains
            .keys()
            .map(|capability_domain_id| (*capability_domain_id).to_string())
            .collect::<BTreeSet<_>>();
        default_registry().validate_in_capability_domains(action_id, args, &capability_domain_ids)
    }

    pub(crate) fn validate_in_capability_domains(
        &self,
        action_id: &str,
        args: &Value,
        capability_domain_ids: &BTreeSet<String>,
    ) -> Result<String, String> {
        let canonical_action_id = Self::canonicalize_action_id(action_id)
            .ok_or_else(|| format!("unknown action `{action_id}`"))?;
        let Some(entry) = self.inner.actions.get(&canonical_action_id) else {
            return Err(format!("unknown action `{action_id}`"));
        };
        if !capability_domain_ids.contains(entry.capability_domain_id) {
            return Err(format!(
                "action `{canonical_action_id}` is not available in this session"
            ));
        }
        validate_execution_mode_arg(&canonical_action_id, args, entry.action.spec().mode_support)?;
        let normalized_args = strip_execution_control_fields_from_args(args);
        entry.action.validate(&normalized_args)?;
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
        capability_domain_state: &Value,
        execution_timeout_ms: u64,
    ) -> Option<ActionOutcome> {
        let execution_args_json = strip_execution_control_fields_from_args_json(args_json);
        match resolved.capability_domain_id.as_str() {
            fathom_capability_domain_fs::FILESYSTEM_CAPABILITY_DOMAIN_ID => {
                fathom_capability_domain_fs::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    capability_domain_state,
                )
            }
            fathom_capability_domain_brave_search::BRAVE_SEARCH_CAPABILITY_DOMAIN_ID => {
                fathom_capability_domain_brave_search::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    capability_domain_state,
                    execution_timeout_ms,
                )
                .await
            }
            fathom_capability_domain_jina::JINA_CAPABILITY_DOMAIN_ID => {
                fathom_capability_domain_jina::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    capability_domain_state,
                    execution_timeout_ms,
                )
                .await
            }
            fathom_capability_domain_shell::SHELL_CAPABILITY_DOMAIN_ID => {
                fathom_capability_domain_shell::execute_action(
                    resolved.action_name.as_str(),
                    &execution_args_json,
                    capability_domain_state,
                    execution_timeout_ms,
                )
                .await
            }
            "system" => {
                crate::system_capability_domain::execute_action(
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
        let mut capability_domains: BTreeMap<&'static str, Arc<dyn CapabilityDomain>> =
            BTreeMap::new();

        register_capability_domain(
            &mut capability_domains,
            Arc::new(fathom_capability_domain_fs::FilesystemCapabilityDomain),
        );
        register_capability_domain(
            &mut capability_domains,
            Arc::new(fathom_capability_domain_brave_search::BraveSearchCapabilityDomain),
        );
        register_capability_domain(
            &mut capability_domains,
            Arc::new(fathom_capability_domain_jina::JinaCapabilityDomain),
        );
        register_capability_domain(
            &mut capability_domains,
            Arc::new(fathom_capability_domain_shell::ShellCapabilityDomain),
        );
        register_capability_domain(&mut capability_domains, Arc::new(SystemCapabilityDomain));

        let mut actions: BTreeMap<String, RegisteredAction> = BTreeMap::new();
        for environment in capability_domains.values() {
            for action in environment.actions() {
                let spec = action.spec();
                let canonical_action_id =
                    canonical_action_id(spec.capability_domain_id, spec.action_name);
                let entry = RegisteredAction {
                    canonical_action_id: canonical_action_id.clone(),
                    capability_domain_id: spec.capability_domain_id,
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
            inner: Arc::new(CapabilityDomainRegistryInner {
                capability_domains,
                actions,
            }),
        }
    }

    fn resolve_by_canonical_id(&self, canonical_action_id: &str) -> Option<ResolvedAction> {
        let entry = self.inner.actions.get(canonical_action_id)?;
        Some(ResolvedAction {
            canonical_action_id: entry.canonical_action_id.clone(),
            capability_domain_id: entry.capability_domain_id.to_string(),
            action_name: entry.action_name.to_string(),
            mode_support: entry.action.spec().mode_support,
            environment: entry.environment.clone(),
            timeout_policy: entry.timeout_policy.clone(),
        })
    }

    fn lookup_capability_domain_summary(
        &self,
        capability_domain_id: &str,
    ) -> Option<ActivatedCapabilityDomainSummary> {
        let environment = self.inner.capability_domains.get(capability_domain_id)?;
        let spec = environment.spec();
        Some(ActivatedCapabilityDomainSummary {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            recipes: environment.recipes(),
        })
    }

    fn lookup_capability_domain_action_summaries(
        &self,
        capability_domain_id: &str,
    ) -> Option<Vec<CapabilityDomainActionSummary>> {
        if !self
            .inner
            .capability_domains
            .contains_key(capability_domain_id)
        {
            return None;
        }

        let actions = self
            .inner
            .actions
            .values()
            .filter(|entry| entry.capability_domain_id == capability_domain_id)
            .map(|entry| {
                let spec = entry.action.spec();
                CapabilityDomainActionSummary {
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

const ACTION_EXECUTION_MODE_KEY: &str = "execution_mode";

fn with_runtime_action_schema(input_schema: Value, mode_support: ActionModeSupport) -> Value {
    let Value::Object(mut schema) = input_schema else {
        return input_schema;
    };
    ensure_schema_object_properties(&mut schema, mode_support);
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

fn strip_execution_control_fields_from_args_json(args_json: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(args_json) else {
        return args_json.to_string();
    };
    let value = strip_execution_control_fields_from_args(&value);
    serde_json::to_string(&value).unwrap_or_else(|_| args_json.to_string())
}

fn strip_execution_control_fields_from_args(args: &Value) -> Value {
    let mut value = args.clone();
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    object.remove(ACTION_EXECUTION_MODE_KEY);
    value
}

fn default_registry() -> &'static CapabilityDomainRegistry {
    static REGISTRY: OnceLock<CapabilityDomainRegistry> = OnceLock::new();
    REGISTRY.get_or_init(CapabilityDomainRegistry::build)
}

fn register_capability_domain(
    capability_domains: &mut BTreeMap<&'static str, Arc<dyn CapabilityDomain>>,
    environment: Arc<dyn CapabilityDomain>,
) {
    let id = environment.spec().id;
    let old = capability_domains.insert(id, environment);
    assert!(
        old.is_none(),
        "duplicate environment registration for `{id}`"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use fathom_capability_domain::ActionModeSupport;
    use serde_json::json;

    use super::{
        ActionTimeoutPolicy, CapabilityDomainRegistry, RequestedExecutionMode,
        requested_execution_mode_from_args_json, validate_execution_mode_arg,
    };

    #[test]
    fn known_actions_align_with_openai_definitions() {
        let registry = CapabilityDomainRegistry::new();
        let names = CapabilityDomainRegistry::known_action_ids();
        let definitions = registry.openai_action_definitions();

        assert_eq!(definitions.len(), names.len());
    }

    #[test]
    fn validates_filesystem_relative_path() {
        let invalid = CapabilityDomainRegistry::validate(
            "filesystem__read",
            &json!({"path":"fs://notes.txt"}),
        );
        assert!(invalid.is_err());

        let valid =
            CapabilityDomainRegistry::validate("filesystem__read", &json!({"path":"notes.txt"}));
        assert!(valid.is_ok());
    }

    #[test]
    fn openai_definitions_do_not_require_debug_field() {
        let registry = CapabilityDomainRegistry::new();
        let definitions = registry.openai_action_definitions();

        for definition in definitions {
            let required = definition["parameters"]["required"]
                .as_array()
                .expect("required must be an array");
            assert!(
                required
                    .iter()
                    .all(|entry| entry.as_str() != Some("reasoning")),
                "debug field must not be required for {}",
                definition["name"]
            );
        }
    }

    #[test]
    fn activated_capability_domain_summaries_include_name_and_description() {
        let summaries = CapabilityDomainRegistry::activated_capability_domain_summaries(&[
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
        let registry = CapabilityDomainRegistry::new();
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
    fn shell_run_definition_exists() {
        let registry = CapabilityDomainRegistry::new();
        let definitions = registry.openai_action_definitions();

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("shell__run"))
        );
    }

    #[test]
    fn brave_search_web_search_definition_exists() {
        let registry = CapabilityDomainRegistry::new();
        let definitions = registry.openai_action_definitions();

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("brave_search__web_search"))
        );
    }

    #[test]
    fn jina_read_url_definition_exists() {
        let registry = CapabilityDomainRegistry::new();
        let definitions = registry.openai_action_definitions();

        assert!(
            definitions
                .iter()
                .any(|definition| definition["name"] == json!("jina__read_url"))
        );
    }

    #[test]
    fn filtered_openai_definitions_only_include_allowed_capability_domains() {
        let registry = CapabilityDomainRegistry::new();
        let definitions =
            registry.openai_action_definitions_for_capability_domains(&BTreeSet::from([
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
    fn validate_in_capability_domains_rejects_action_outside_allowed_session_set() {
        let registry = CapabilityDomainRegistry::new();
        let allowed = BTreeSet::from(["filesystem".to_string()]);

        let error = registry
            .validate_in_capability_domains("shell__run", &json!({"command":"pwd"}), &allowed)
            .expect_err("shell action should be rejected when shell is not engaged");
        assert!(error.contains("is not available in this session"));
    }

    #[test]
    fn registered_actions_have_valid_timeout_policies() {
        let action_ids = CapabilityDomainRegistry::known_action_ids();
        for action_id in action_ids {
            let resolved = CapabilityDomainRegistry::resolve(&action_id)
                .expect("registered action should resolve");
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
    fn strip_execution_control_fields_preserves_regular_arguments() {
        let raw = r#"{"query":"seti","count":5}"#;
        let stripped = super::strip_execution_control_fields_from_args_json(raw);
        let value: serde_json::Value =
            serde_json::from_str(&stripped).expect("stripped args must be valid json");
        assert_eq!(value["query"], "seti");
        assert_eq!(value["count"], 5);
    }

    #[test]
    fn strips_execution_mode_field_for_execution() {
        let raw = r#"{"query":"seti","count":5,"execution_mode":"detach"}"#;
        let stripped = super::strip_execution_control_fields_from_args_json(raw);
        let value: serde_json::Value =
            serde_json::from_str(&stripped).expect("stripped args must be valid json");
        assert!(value.get("execution_mode").is_none());
        assert_eq!(value["query"], "seti");
        assert_eq!(value["count"], 5);
    }

    #[test]
    fn requested_execution_mode_defaults_to_await() {
        let mode = requested_execution_mode_from_args_json(r#"{"query":"seti"}"#)
            .expect("mode should parse");
        assert_eq!(mode, RequestedExecutionMode::Await);
    }

    #[test]
    fn requested_execution_mode_accepts_detach() {
        let mode = requested_execution_mode_from_args_json(
            r#"{"query":"seti","execution_mode":"detach"}"#,
        )
        .expect("mode should parse");
        assert_eq!(mode, RequestedExecutionMode::Detach);
    }

    #[test]
    fn requested_execution_mode_rejects_invalid_values() {
        let error =
            requested_execution_mode_from_args_json(r#"{"query":"seti","execution_mode":"later"}"#)
                .expect_err("invalid mode must be rejected");
        assert!(error.contains("must be `await` or `detach`"));
    }

    #[test]
    fn validate_execution_mode_allows_omitted_mode() {
        let result = validate_execution_mode_arg(
            "filesystem__list",
            &json!({"path":"."}),
            ActionModeSupport::AwaitOnly,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_execution_mode_rejects_detach_for_await_only_action() {
        let error = validate_execution_mode_arg(
            "filesystem__list",
            &json!({"path":".","execution_mode":"detach"}),
            ActionModeSupport::AwaitOnly,
        )
        .expect_err("detach must be rejected for await_only actions");
        assert!(error.contains("detach is not allowed"));
    }

    #[test]
    fn validate_execution_mode_accepts_detach_for_detach_capable_action() {
        let result = validate_execution_mode_arg(
            "system__get_context",
            &json!({"execution_mode":"detach"}),
            ActionModeSupport::AwaitOrDetach,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn openai_definition_limits_execution_mode_enum_for_await_only_action() {
        let registry = CapabilityDomainRegistry::new();
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
        let registry = CapabilityDomainRegistry::new();
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
    fn validation_strips_execution_mode_before_action_validation() {
        let valid = CapabilityDomainRegistry::validate(
            "system__get_time",
            &json!({"execution_mode":"await"}),
        );
        assert!(valid.is_ok());
    }

    #[test]
    fn validation_accepts_actions_without_debug_field() {
        let valid =
            CapabilityDomainRegistry::validate("filesystem__read", &json!({"path":"notes.txt"}));
        assert!(valid.is_ok());
    }
}
