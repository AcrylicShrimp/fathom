use serde_json::{Value, json};

use crate::environment::EnvironmentRegistry;

pub(crate) fn describe_environment(env_id: &str) -> Option<Value> {
    let environment = EnvironmentRegistry::environment_summary(env_id)?;
    let actions = EnvironmentRegistry::environment_action_summaries(env_id)?;

    Some(json!({
        "id": environment.id,
        "name": environment.name,
        "description": environment.description,
        "intended_for": intended_for(env_id),
        "capabilities": capabilities_for(env_id),
        "actions": actions.into_iter().map(|action| {
            json!({
                "id": action.id,
                "name": action.name,
                "description": action.description,
                "discovery": action.discovery,
                "input_schema": action.input_schema,
            })
        }).collect::<Vec<_>>(),
        "recipes": recipes_for(env_id),
    }))
}

fn intended_for(env_id: &str) -> &'static str {
    match env_id {
        "filesystem" => {
            "Working with files and directories under the session's filesystem base_path."
        }
        "system" => "Inspecting runtime/session context, profiles, and task payloads.",
        _ => "General environment-specific operations.",
    }
}

fn capabilities_for(env_id: &str) -> Vec<&'static str> {
    match env_id {
        "filesystem" => vec![
            "Read and write files relative to base_path",
            "List directories and inspect file content",
            "Apply text replacement in file content",
            "Expose current base_path through inspection action",
        ],
        "system" => vec![
            "Query canonical runtime/session context",
            "Inspect current server time and timezone",
            "Inspect profile metadata and full profile content",
            "Load full task args/result payloads from previews",
            "Describe activated environments and their action inventory",
        ],
        _ => vec!["Inspect environment capabilities and action contracts"],
    }
}

fn recipes_for(env_id: &str) -> Vec<Value> {
    match env_id {
        "filesystem" => vec![
            json!({
                "title": "Find and read a file",
                "steps": [
                    "Call filesystem__get_base_path to confirm scope.",
                    "Call filesystem__list with path '.' or a relative directory.",
                    "Call filesystem__read with a relative file path from the listing."
                ],
            }),
            json!({
                "title": "Create or update file content",
                "steps": [
                    "Call filesystem__write with {path, content, allow_override}.",
                    "Call filesystem__read to verify the final content.",
                    "If you need targeted edits, call filesystem__replace with mode first/all."
                ],
            }),
        ],
        "system" => vec![
            json!({
                "title": "Refresh runtime context",
                "steps": [
                    "Call system__get_context to load current runtime/session context and activated environments.",
                    "Call system__get_time when fresher clock data is required mid-turn."
                ],
            }),
            json!({
                "title": "Expand task preview into full payload",
                "steps": [
                    "Read task_started/task_finished history preview.",
                    "Call system__get_task_payload with {task_id, part}.",
                    "Continue planning with full args/result content."
                ],
            }),
        ],
        _ => vec![json!({
            "title": "Inspect environment details",
            "steps": [
                "Review actions and schemas in the returned description payload."
            ],
        })],
    }
}
