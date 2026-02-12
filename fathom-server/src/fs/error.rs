#[derive(Debug, Clone)]
pub(crate) struct FsError {
    code: &'static str,
    message: String,
}

impl FsError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
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

    pub(crate) fn not_file(message: impl Into<String>) -> Self {
        Self::new("not_file", message)
    }

    pub(crate) fn not_directory(message: impl Into<String>) -> Self {
        Self::new("not_directory", message)
    }

    pub(crate) fn already_exists(message: impl Into<String>) -> Self {
        Self::new("already_exists", message)
    }

    pub(crate) fn permission_denied(message: impl Into<String>) -> Self {
        Self::new("permission_denied", message)
    }

    pub(crate) fn io_error(message: impl Into<String>) -> Self {
        Self::new("io_error", message)
    }

    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}
