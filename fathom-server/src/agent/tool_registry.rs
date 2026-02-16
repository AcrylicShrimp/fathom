use serde_json::{Value, json};

use crate::agent::tools::{
    ToolCatalogEntry, all_tools, discovery_tool_names, known_tool_names, parameters_for,
    validate_tool_args,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct ToolRegistry;

impl ToolRegistry {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn openai_tool_definitions(&self) -> Vec<Value> {
        all_tools()
            .iter()
            .filter_map(openai_tool_definition)
            .collect()
    }

    pub(crate) fn validate(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        validate_tool_args(tool_name, args)
    }

    pub(crate) fn known_tool_names() -> Vec<String> {
        known_tool_names()
    }

    pub(crate) fn discovery_tool_names() -> Vec<String> {
        discovery_tool_names()
    }
}

fn openai_tool_definition(tool: &ToolCatalogEntry) -> Option<Value> {
    let parameters = parameters_for(tool.name)?;
    Some(json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": parameters,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ToolRegistry;

    #[test]
    fn known_tools_align_with_openai_definitions() {
        let registry = ToolRegistry::new();
        let names = ToolRegistry::known_tool_names();
        let definitions = registry.openai_tool_definitions();

        assert_eq!(definitions.len(), names.len());
    }

    #[test]
    fn validates_fs_path_prefix() {
        let registry = ToolRegistry::new();
        let invalid = registry.validate("fs_read", &json!({"path":"./no-scheme.txt"}));
        assert!(invalid.is_err());

        let valid = registry.validate("fs_read", &json!({"path":"fs://notes.txt"}));
        assert!(valid.is_ok());
    }
}
