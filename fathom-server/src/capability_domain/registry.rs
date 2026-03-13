use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Map, Value, json};

use fathom_capability_domain::{
    CapabilityActionDefinition, CapabilityActionKey, CapabilityDomainRecipe, DomainFactory,
    canonical_action_id, parse_action_id,
};

#[derive(Clone)]
pub(crate) struct ResolvedAction {
    pub(crate) capability_domain_id: String,
    pub(crate) action_key: CapabilityActionKey,
}

#[derive(Clone)]
pub(crate) struct CapabilityDomainRegistry {
    inner: Arc<CapabilityDomainRegistryInner>,
}

struct CapabilityDomainRegistryInner {
    domain_factories: BTreeMap<&'static str, Arc<dyn DomainFactory>>,
    actions: BTreeMap<String, RegisteredAction>,
}

struct RegisteredAction {
    canonical_action_id: String,
    capability_domain_id: &'static str,
    definition: CapabilityActionDefinition,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ActivatedCapabilityDomainSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) recipes: Vec<CapabilityDomainRecipe>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityDomainActionSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) input_schema: Value,
}

impl CapabilityDomainRegistry {
    pub(crate) fn from_domain_factories(domain_factories: Vec<Arc<dyn DomainFactory>>) -> Self {
        let mut domain_factory_map: BTreeMap<&'static str, Arc<dyn DomainFactory>> =
            BTreeMap::new();
        for domain_factory in domain_factories {
            register_domain_factory(&mut domain_factory_map, domain_factory);
        }

        let mut actions = BTreeMap::new();
        for domain_factory in domain_factory_map.values() {
            let spec = domain_factory.spec();
            for definition in domain_factory.actions() {
                let canonical_action_id = canonical_action_id(spec.id, definition.action_name);
                let old = actions.insert(
                    canonical_action_id.clone(),
                    RegisteredAction {
                        canonical_action_id,
                        capability_domain_id: spec.id,
                        definition,
                    },
                );
                assert!(
                    old.is_none(),
                    "duplicate action registration for `{}`",
                    spec.id
                );
            }
        }

        Self {
            inner: Arc::new(CapabilityDomainRegistryInner {
                domain_factories: domain_factory_map,
                actions,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn openai_action_definitions(&self) -> Vec<Value> {
        let domain_ids = self
            .inner
            .domain_factories
            .keys()
            .map(|id| (*id).to_string())
            .collect::<BTreeSet<_>>();
        self.openai_action_definitions_for_capability_domains(&domain_ids)
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
                json!({
                    "type": "function",
                    "name": entry.canonical_action_id,
                    "description": entry.definition.description,
                    "parameters": with_runtime_action_schema(entry.definition.input_schema.clone()),
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn known_action_ids(&self) -> Vec<String> {
        self.inner.actions.keys().cloned().collect()
    }

    #[cfg(test)]
    pub(crate) fn activated_capability_domain_summaries(
        &self,
        capability_domain_ids: &[String],
    ) -> Vec<ActivatedCapabilityDomainSummary> {
        capability_domain_ids
            .iter()
            .filter_map(|capability_domain_id| {
                self.lookup_capability_domain_summary(capability_domain_id)
            })
            .collect()
    }

    pub(crate) fn capability_domain_summary(
        &self,
        capability_domain_id: &str,
    ) -> Option<ActivatedCapabilityDomainSummary> {
        self.lookup_capability_domain_summary(capability_domain_id)
    }

    pub(crate) fn capability_domain_action_summaries(
        &self,
        capability_domain_id: &str,
    ) -> Option<Vec<CapabilityDomainActionSummary>> {
        self.lookup_capability_domain_action_summaries(capability_domain_id)
    }

    pub(crate) fn canonicalize_action_id(action_id: &str) -> Option<String> {
        let (capability_domain_id, action_name) = parse_action_id(action_id)?;
        Some(canonical_action_id(&capability_domain_id, &action_name))
    }

    #[cfg(test)]
    pub(crate) fn validate(&self, action_id: &str, args: &Value) -> Result<String, String> {
        let domain_ids = self
            .inner
            .domain_factories
            .keys()
            .map(|id| (*id).to_string())
            .collect::<BTreeSet<_>>();
        self.validate_in_capability_domains(action_id, args, &domain_ids)
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
        if !args.is_object() {
            return Err("action arguments must be a JSON object".to_string());
        }
        Ok(canonical_action_id)
    }

    pub(crate) fn resolve(&self, action_id: &str) -> Option<ResolvedAction> {
        let canonical_action_id = Self::canonicalize_action_id(action_id)?;
        self.resolve_by_canonical_id(&canonical_action_id)
    }

    pub(crate) fn installed_capability_domain_ids(&self) -> Vec<String> {
        self.inner
            .domain_factories
            .keys()
            .map(|id| (*id).to_string())
            .collect()
    }

    pub(crate) fn domain_factory(
        &self,
        capability_domain_id: &str,
    ) -> Option<Arc<dyn DomainFactory>> {
        self.inner
            .domain_factories
            .get(capability_domain_id)
            .cloned()
    }

    fn resolve_by_canonical_id(&self, canonical_action_id: &str) -> Option<ResolvedAction> {
        let entry = self.inner.actions.get(canonical_action_id)?;
        Some(ResolvedAction {
            capability_domain_id: entry.capability_domain_id.to_string(),
            action_key: entry.definition.key,
        })
    }

    fn lookup_capability_domain_summary(
        &self,
        capability_domain_id: &str,
    ) -> Option<ActivatedCapabilityDomainSummary> {
        let domain_factory = self.inner.domain_factories.get(capability_domain_id)?;
        let spec = domain_factory.spec();
        Some(ActivatedCapabilityDomainSummary {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            recipes: domain_factory.recipes(),
        })
    }

    fn lookup_capability_domain_action_summaries(
        &self,
        capability_domain_id: &str,
    ) -> Option<Vec<CapabilityDomainActionSummary>> {
        if !self
            .inner
            .domain_factories
            .contains_key(capability_domain_id)
        {
            return None;
        }

        Some(
            self.inner
                .actions
                .values()
                .filter(|entry| entry.capability_domain_id == capability_domain_id)
                .map(|entry| CapabilityDomainActionSummary {
                    id: entry.canonical_action_id.clone(),
                    name: entry.definition.action_name.to_string(),
                    description: entry.definition.description.to_string(),
                    input_schema: entry.definition.input_schema.clone(),
                })
                .collect(),
        )
    }
}

const ACTION_BACKGROUND_KEY: &str = "background";

fn with_runtime_action_schema(input_schema: Value) -> Value {
    let Value::Object(mut schema) = input_schema else {
        return input_schema;
    };
    ensure_schema_object_properties(&mut schema);
    Value::Object(schema)
}

fn ensure_schema_object_properties(schema: &mut Map<String, Value>) {
    let properties_entry = schema
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(properties) = properties_entry else {
        return;
    };
    properties
        .entry(ACTION_BACKGROUND_KEY.to_string())
        .or_insert_with(|| {
            json!({
                "type": "boolean",
                "description": "Optional Core scheduling hint. Omit this field or use `false` unless the execution should begin in background."
            })
        });
}

fn register_domain_factory(
    domain_factories: &mut BTreeMap<&'static str, Arc<dyn DomainFactory>>,
    domain_factory: Arc<dyn DomainFactory>,
) {
    let id = domain_factory.spec().id;
    let old = domain_factories.insert(id, domain_factory);
    assert!(
        old.is_none(),
        "duplicate domain factory registration for `{id}`"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::env;

    use serde_json::json;

    use super::CapabilityDomainRegistry;
    use crate::capability_domain::build_default_capability_domain_registry;

    fn test_registry() -> CapabilityDomainRegistry {
        build_default_capability_domain_registry(
            &env::current_dir().expect("current directory for registry"),
        )
    }

    #[test]
    fn known_actions_align_with_openai_definitions() {
        let registry = test_registry();
        let names = registry.known_action_ids();
        let definitions = registry.openai_action_definitions();

        assert_eq!(definitions.len(), names.len());
    }

    #[test]
    fn validate_accepts_object_args_for_known_action() {
        let registry = test_registry();
        let valid = registry.validate("filesystem__read", &json!({"path":"notes.txt"}));
        assert!(valid.is_ok());
    }

    #[test]
    fn openai_definitions_include_background_flag() {
        let registry = test_registry();
        let definition = registry
            .openai_action_definitions()
            .into_iter()
            .find(|definition| definition["name"] == json!("shell__run"))
            .expect("shell__run definition");

        assert_eq!(
            definition["parameters"]["properties"]["background"]["type"],
            json!("boolean")
        );
    }

    #[test]
    fn activated_capability_domain_summaries_include_name_and_description() {
        let registry = test_registry();
        let summaries = registry.activated_capability_domain_summaries(&[
            "filesystem".to_string(),
            "system".to_string(),
        ]);
        assert!(
            summaries
                .iter()
                .any(|summary| summary.id == "filesystem" && !summary.description.is_empty())
        );
        assert!(
            summaries
                .iter()
                .any(|summary| summary.id == "system" && !summary.description.is_empty())
        );
    }

    #[test]
    fn validate_in_capability_domains_rejects_action_outside_allowed_session_set() {
        let registry = test_registry();
        let allowed = BTreeSet::from(["filesystem".to_string()]);

        let error = registry
            .validate_in_capability_domains("shell__run", &json!({"command":"pwd"}), &allowed)
            .expect_err("shell action should not validate outside allowed domains");
        assert!(error.contains("not available in this session"));
    }

    #[test]
    fn resolve_returns_registered_action_metadata() {
        let registry = test_registry();
        let action_ids = registry.known_action_ids();
        for action_id in action_ids {
            registry.resolve(&action_id).expect("action must resolve");
        }
    }
}
