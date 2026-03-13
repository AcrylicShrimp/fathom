use fathom_capability_domain::{CapabilityActionDefinition, CapabilityActionKey};
use serde_json::Value;
pub(super) const SYSTEM_CAPABILITY_DOMAIN_ID: &str = "system";
pub(super) const SYSTEM_LIST_EXECUTIONS_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(0);
pub(super) const SYSTEM_GET_EXECUTION_ACTION_KEY: CapabilityActionKey = CapabilityActionKey(1);
pub(super) const SYSTEM_READ_EXECUTION_INPUT_ACTION_KEY: CapabilityActionKey =
    CapabilityActionKey(2);
pub(super) const SYSTEM_READ_EXECUTION_RESULT_ACTION_KEY: CapabilityActionKey =
    CapabilityActionKey(3);

pub(super) fn system_spec(
    action_key: u16,
    action_name: &'static str,
    description: &'static str,
    input_schema: Value,
) -> CapabilityActionDefinition {
    CapabilityActionDefinition {
        key: CapabilityActionKey(action_key),
        action_name,
        description,
        input_schema,
    }
}
