use chrono::{Local, SecondsFormat, Utc};
use serde_json::{Value, json};

use super::Runtime;

const SERVER_CLOCK_SOURCE: &str = "server_clock";

impl Runtime {
    pub(crate) fn current_system_time_context(&self) -> Value {
        build_current_system_time_context()
    }
}

fn build_current_system_time_context() -> Value {
    let utc_now = Utc::now();
    let utc_rfc3339 = utc_now.to_rfc3339_opts(SecondsFormat::Millis, true);

    let local_now = Local::now();
    let local_rfc3339 = local_now.to_rfc3339_opts(SecondsFormat::Millis, false);
    let local_utc_offset = local_now.format("%:z").to_string();
    let local_timezone_name = resolve_local_timezone_name(&local_now);

    build_system_time_context(
        utc_now.timestamp_millis(),
        utc_rfc3339,
        local_rfc3339,
        local_utc_offset,
        local_timezone_name,
    )
}

fn build_system_time_context(
    generated_at_unix_ms: i64,
    utc_rfc3339: String,
    local_rfc3339: String,
    local_utc_offset: String,
    local_timezone_name: Option<String>,
) -> Value {
    if let Some(local_timezone_name) = normalize_timezone_name(local_timezone_name) {
        return json!({
            "generated_at_unix_ms": generated_at_unix_ms,
            "utc_rfc3339": utc_rfc3339,
            "local_rfc3339": local_rfc3339,
            "local_timezone_name": local_timezone_name,
            "local_utc_offset": normalize_utc_offset(local_utc_offset),
            "time_source": SERVER_CLOCK_SOURCE,
        });
    }

    json!({
        "generated_at_unix_ms": generated_at_unix_ms,
        "utc_rfc3339": utc_rfc3339.clone(),
        "local_rfc3339": utc_rfc3339,
        "local_timezone_name": "UTC",
        "local_utc_offset": "+00:00",
        "time_source": SERVER_CLOCK_SOURCE,
    })
}

fn normalize_timezone_name(value: Option<String>) -> Option<String> {
    value.and_then(|name| {
        let normalized = name.trim();
        (!normalized.is_empty()).then(|| normalized.to_string())
    })
}

fn normalize_utc_offset(value: String) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "+00:00".to_string()
    } else {
        normalized.to_string()
    }
}

fn resolve_local_timezone_name(local_now: &chrono::DateTime<Local>) -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.trim();
        if !tz.is_empty() {
            return Some(tz.to_string());
        }
    }

    let inferred = local_now.format("%Z").to_string();
    let inferred = inferred.trim();
    if inferred.is_empty() {
        None
    } else {
        Some(inferred.to_string())
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::{build_current_system_time_context, build_system_time_context};

    #[test]
    fn current_time_context_uses_rfc3339_fields() {
        let context = build_current_system_time_context();

        assert!(
            DateTime::parse_from_rfc3339(context["utc_rfc3339"].as_str().expect("utc")).is_ok()
        );
        assert!(
            DateTime::parse_from_rfc3339(context["local_rfc3339"].as_str().expect("local")).is_ok()
        );
        assert!(
            !context["local_timezone_name"]
                .as_str()
                .expect("tz")
                .trim()
                .is_empty()
        );
        assert!(
            !context["local_utc_offset"]
                .as_str()
                .expect("offset")
                .trim()
                .is_empty()
        );
        assert_eq!(context["time_source"], "server_clock");
    }

    #[test]
    fn missing_timezone_name_falls_back_to_utc() {
        let utc = "2026-02-16T00:00:00.000Z".to_string();
        let context = build_system_time_context(
            1_765_000_000_000,
            utc.clone(),
            "2026-02-16T09:00:00.000+09:00".to_string(),
            "+09:00".to_string(),
            None,
        );

        assert_eq!(context["utc_rfc3339"], utc.clone());
        assert_eq!(context["local_rfc3339"], utc);
        assert_eq!(context["local_timezone_name"], "UTC");
        assert_eq!(context["local_utc_offset"], "+00:00");
    }
}
