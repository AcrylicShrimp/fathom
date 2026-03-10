pub fn canonical_action_id(capability_domain_id: &str, action_name: &str) -> String {
    format!("{capability_domain_id}__{action_name}")
}

pub fn parse_action_id(raw: &str) -> Option<(String, String)> {
    let mut segments = raw.splitn(2, "__");
    let capability_domain_id = segments.next()?.trim();
    let action_name = segments.next()?.trim();
    if capability_domain_id.is_empty() || action_name.is_empty() {
        return None;
    }

    Some((capability_domain_id.to_string(), action_name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{canonical_action_id, parse_action_id};

    #[test]
    fn parses_canonical_action_id() {
        let parsed = parse_action_id("filesystem__read").expect("valid action id");
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
