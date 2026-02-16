use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct ShellError {
    code: &'static str,
    message: String,
    details: Option<Value>,
}

impl ShellError {
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

    pub(crate) fn invalid_path(message: impl Into<String>) -> Self {
        Self::new("invalid_path", message)
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub(crate) fn not_directory(message: impl Into<String>) -> Self {
        Self::new("not_directory", message)
    }

    pub(crate) fn permission_denied(message: impl Into<String>) -> Self {
        Self::new("permission_denied", message)
    }

    pub(crate) fn io_error(message: impl Into<String>) -> Self {
        Self::new("io_error", message)
    }

    pub(crate) fn spawn_failed(message: impl Into<String>) -> Self {
        Self::new("spawn_failed", message)
    }

    pub(crate) fn timeout(message: impl Into<String>) -> Self {
        Self::new("timeout", message)
    }

    pub(crate) fn execution_failed(message: impl Into<String>) -> Self {
        Self::new("execution_failed", message)
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
