pub struct LegacyActionAlias {
    pub alias: &'static str,
    pub environment_id: &'static str,
    pub action_name: &'static str,
}

pub fn canonical_action_id(environment_id: &str, action_name: &str) -> String {
    format!("{environment_id}__{action_name}")
}

pub fn parse_action_id(raw: &str) -> Option<(String, String)> {
    let mut segments = raw.splitn(2, "__");
    let environment_id = segments.next()?.trim();
    let action_name = segments.next()?.trim();
    if environment_id.is_empty() || action_name.is_empty() {
        return None;
    }

    Some((environment_id.to_string(), action_name.to_string()))
}

pub fn parse_action_id_with_aliases(
    raw: &str,
    aliases: &[LegacyActionAlias],
) -> Option<(String, String)> {
    if let Some((environment_id, action_name)) = parse_action_id(raw) {
        return Some((environment_id, action_name));
    }

    aliases
        .iter()
        .find(|alias| alias.alias == raw)
        .map(|alias| {
            (
                alias.environment_id.to_string(),
                alias.action_name.to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{
        LegacyActionAlias, canonical_action_id, parse_action_id, parse_action_id_with_aliases,
    };

    #[test]
    fn parses_canonical_action_id() {
        let parsed = parse_action_id("filesystem__read").expect("valid action id");
        assert_eq!(parsed.0, "filesystem");
        assert_eq!(parsed.1, "read");
    }

    #[test]
    fn resolves_aliases() {
        let aliases = [LegacyActionAlias {
            alias: "fs_read",
            environment_id: "filesystem",
            action_name: "read",
        }];
        let parsed = parse_action_id_with_aliases("fs_read", &aliases).expect("alias parse");
        assert_eq!(parsed.0, "filesystem");
        assert_eq!(parsed.1, "read");
    }

    #[test]
    fn renders_canonical_action_id() {
        assert_eq!(
            canonical_action_id("system", "get_time"),
            "system__get_time"
        );
    }
}
