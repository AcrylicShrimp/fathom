#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedCommand {
    pub(crate) name: String,
    pub(crate) args: String,
}

pub(crate) fn parse_slash_command(input: &str) -> Option<ParsedCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let remainder = trimmed.trim_start_matches('/');
    if remainder.is_empty() {
        return Some(ParsedCommand {
            name: String::new(),
            args: String::new(),
        });
    }

    let split_index = remainder
        .find(char::is_whitespace)
        .unwrap_or(remainder.len());
    let name = remainder[..split_index].to_ascii_lowercase();
    let args = remainder[split_index..].trim().to_string();

    Some(ParsedCommand { name, args })
}

pub(crate) fn completion_query(input: &str) -> Option<&str> {
    if !input.starts_with('/') {
        return None;
    }

    let remainder = &input[1..];
    if remainder.chars().any(char::is_whitespace) {
        return None;
    }

    Some(remainder)
}

#[cfg(test)]
mod tests {
    use super::{completion_query, parse_slash_command};

    #[test]
    fn parses_slash_command_with_optional_args() {
        let parsed = parse_slash_command("/heartbeat now").expect("slash command");
        assert_eq!(parsed.name, "heartbeat");
        assert_eq!(parsed.args, "now");
    }

    #[test]
    fn parses_slash_command_without_args() {
        let parsed = parse_slash_command(" /heartbeat   ").expect("slash command");
        assert_eq!(parsed.name, "heartbeat");
        assert!(parsed.args.is_empty());
    }

    #[test]
    fn parse_returns_none_for_non_slash_input() {
        assert!(parse_slash_command("hello").is_none());
    }

    #[test]
    fn completion_query_requires_leading_slash_without_whitespace() {
        assert_eq!(completion_query("/"), Some(""));
        assert_eq!(completion_query("/he"), Some("he"));
        assert_eq!(completion_query("/he now"), None);
        assert_eq!(completion_query("he"), None);
    }
}
