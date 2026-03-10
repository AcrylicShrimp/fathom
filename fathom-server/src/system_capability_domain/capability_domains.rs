use serde_json::{Value, json};

use crate::capability_domain::CapabilityDomainRegistry;

pub(crate) fn describe_capability_domain(capability_domain_id: &str) -> Option<Value> {
    let environment = CapabilityDomainRegistry::capability_domain_summary(capability_domain_id)?;
    let actions =
        CapabilityDomainRegistry::capability_domain_action_summaries(capability_domain_id)?;
    let recipes = environment
        .recipes
        .iter()
        .map(|recipe| {
            json!({
                "title": recipe.title,
                "steps": recipe.steps,
            })
        })
        .collect::<Vec<_>>();

    Some(json!({
        "id": environment.id,
        "name": environment.name,
        "description": environment.description,
        "intended_for": intended_for(capability_domain_id),
        "capabilities": capabilities_for(capability_domain_id),
        "actions": actions.into_iter().map(|action| {
            json!({
                "id": action.id,
                "name": action.name,
                "description": action.description,
                "discovery": action.discovery,
                "mode_support": action.mode_support.as_str(),
                "input_schema": action.input_schema,
            })
        }).collect::<Vec<_>>(),
        "recipes": recipes,
    }))
}

fn intended_for(capability_domain_id: &str) -> &'static str {
    match capability_domain_id {
        "filesystem" => {
            "Working with files and directories under the session's filesystem base_path."
        }
        "brave_search" => "Searching the public web via Brave Search API.",
        "jina" => "Reading and extracting webpage content via Jina Reader API.",
        "shell" => "Executing non-interactive shell commands under the session's shell base_path.",
        "system" => "Inspecting runtime/session context, profiles, and task payloads.",
        _ => "General environment-specific operations.",
    }
}

fn capabilities_for(capability_domain_id: &str) -> Vec<&'static str> {
    match capability_domain_id {
        "filesystem" => vec![
            "Read and write files relative to base_path",
            "List directories (optionally recursive) and inspect file content",
            "Search files by glob pattern and regex content match",
            "Apply text replacement in UTF-8 file content",
            "Expose current base_path through inspection action",
            "Enforce non-empty relative path arguments (use `.` to target root)",
            "Return invalid_encoding when read/replace/search targets non-UTF-8 text",
        ],
        "brave_search" => vec![
            "Run web searches against Brave Search API using server-side credentials",
            "Return compact ranked metadata (title, url, description, optional age/source)",
            "Bound result count and enforce strict JSON argument validation",
            "Expose provider/network failures as structured task errors",
        ],
        "jina" => vec![
            "Read one absolute http(s) URL through Jina Reader API",
            "Return extracted markdown content with source metadata",
            "Apply output-size truncation with explicit truncated/truncated_bytes metadata",
            "Expose provider/auth/network failures as structured task errors",
        ],
        "shell" => vec![
            "Run non-interactive shell commands under base_path-relative working directories",
            "Override command environment variables per invocation",
            "Apply per-call timeout limits",
            "Capture stdout/stderr with truncation metadata when output is large",
            "Treat non-zero process exits as failed task outcomes",
        ],
        "system" => vec![
            "Query canonical runtime/session context",
            "Inspect current server time and timezone",
            "Inspect profile metadata and full profile content",
            "Load full task args/result payloads from previews",
            "Describe activated capability_domains and their action inventory",
        ],
        _ => vec!["Inspect environment capabilities and action contracts"],
    }
}
