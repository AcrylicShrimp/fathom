use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityActionKey(pub u16);

#[derive(Debug, Clone)]
pub struct CapabilityActionDefinition {
    pub key: CapabilityActionKey,
    pub action_name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct CapabilityActionSubmission {
    pub action_key: CapabilityActionKey,
    pub args: Value,
}
