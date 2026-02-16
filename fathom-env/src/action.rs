use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ActionSpec {
    pub environment_id: &'static str,
    pub action_name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub discovery: bool,
    pub max_timeout_ms: u64,
    pub desired_timeout_ms: Option<u64>,
}

impl ActionSpec {
    pub fn canonical_id(&self) -> String {
        format!("{}__{}", self.environment_id, self.action_name)
    }

    pub fn effective_timeout_ms(&self) -> Result<u64, String> {
        if self.max_timeout_ms == 0 {
            return Err(format!(
                "invalid timeout policy for `{}`: max_timeout_ms must be > 0",
                self.canonical_id()
            ));
        }

        let timeout_ms = self.desired_timeout_ms.unwrap_or(self.max_timeout_ms);
        if timeout_ms == 0 {
            return Err(format!(
                "invalid timeout policy for `{}`: desired_timeout_ms must be > 0 when set",
                self.canonical_id()
            ));
        }
        if timeout_ms > self.max_timeout_ms {
            return Err(format!(
                "invalid timeout policy for `{}`: desired_timeout_ms ({timeout_ms}) exceeds max_timeout_ms ({})",
                self.canonical_id(),
                self.max_timeout_ms
            ));
        }

        Ok(timeout_ms)
    }
}

pub trait Action: Send + Sync {
    fn spec(&self) -> ActionSpec;

    fn validate(&self, args: &Value) -> Result<(), String>;
}
