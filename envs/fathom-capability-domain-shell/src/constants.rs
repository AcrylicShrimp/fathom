pub(crate) const ACTION_MAX_TIMEOUT_MS: u64 = 60_000;
pub(crate) const ACTION_DESIRED_TIMEOUT_MS: u64 = 20_000;
pub(crate) const DEFAULT_MAX_STDOUT_BYTES: usize = 65_536;
pub(crate) const DEFAULT_MAX_STDERR_BYTES: usize = 65_536;
pub(crate) const MAX_ENV_VARS: usize = 128;
pub(crate) const MAX_COMMAND_BYTES: usize = 16_384;

pub(crate) fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
