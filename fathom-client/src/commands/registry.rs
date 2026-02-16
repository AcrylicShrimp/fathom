use super::heartbeat;
use super::spec::CommandSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandId {
    Heartbeat,
}

const COMMANDS: [(CommandId, CommandSpec); 1] = [(CommandId::Heartbeat, heartbeat::SPEC)];

pub(crate) fn completion_items(prefix: &str) -> Vec<CommandSpec> {
    let normalized = prefix.to_ascii_lowercase();
    COMMANDS
        .iter()
        .filter_map(|(_, spec)| spec.name.starts_with(normalized.as_str()).then_some(*spec))
        .collect()
}

pub(crate) fn resolve(name: &str) -> Option<CommandId> {
    COMMANDS
        .iter()
        .find_map(|(command_id, spec)| spec.name.eq_ignore_ascii_case(name).then_some(*command_id))
}

#[cfg(test)]
mod tests {
    use super::{CommandId, completion_items, resolve};

    #[test]
    fn filters_command_completions_by_prefix() {
        let all = completion_items("");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "heartbeat");

        let filtered = completion_items("hea");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "heartbeat");

        assert!(completion_items("zzz").is_empty());
    }

    #[test]
    fn resolves_commands_case_insensitively() {
        assert_eq!(resolve("heartbeat"), Some(CommandId::Heartbeat));
        assert_eq!(resolve("HEARTBEAT"), Some(CommandId::Heartbeat));
        assert_eq!(resolve("hb"), None);
    }
}
