use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct BraveError {
    code: &'static str,
    message: String,
    details: Option<Value>,
}

impl BraveError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub(crate) fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub(crate) fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("invalid_args", message)
    }

    pub(crate) fn auth_missing(message: impl Into<String>) -> Self {
        Self::new("auth_missing", message)
    }

    pub(crate) fn provider_http(message: impl Into<String>) -> Self {
        Self::new("provider_http", message)
    }

    pub(crate) fn provider_parse(message: impl Into<String>) -> Self {
        Self::new("provider_parse", message)
    }

    pub(crate) fn network(message: impl Into<String>) -> Self {
        Self::new("network", message)
    }

    pub(crate) fn timeout(message: impl Into<String>) -> Self {
        Self::new("timeout", message)
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }

    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn details(&self) -> Option<&Value> {
        self.details.as_ref()
    }
}
