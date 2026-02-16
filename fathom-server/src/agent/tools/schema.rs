use serde_json::{Value, json};

pub(crate) fn parameters_for(tool_name: &str) -> Option<Value> {
    match tool_name {
        "memory_append" => Some(json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "enum": ["agent", "user"] },
                "target_id": { "type": "string" },
                "note": { "type": "string" }
            },
            "required": ["target", "target_id", "note"],
            "additionalProperties": false
        })),
        "refresh_profile" => Some(json!({
            "type": "object",
            "properties": {
                "scope": { "type": "string", "enum": ["agent", "user", "all"] },
                "user_id": {
                    "type": "string",
                    "description": "Required and non-empty when scope=user; otherwise pass an empty string."
                }
            },
            "required": ["scope", "user_id"],
            "additionalProperties": false
        })),
        "schedule_heartbeat" => Some(json!({
            "type": "object",
            "properties": {
                "delay_ms": { "type": "integer", "minimum": 0 }
            },
            "required": ["delay_ms"],
            "additionalProperties": false
        })),
        "send_message" => Some(json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "user_id": { "type": "string" }
            },
            "required": ["content"],
            "additionalProperties": false
        })),
        "fs_list" | "fs_read" => Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"],
            "additionalProperties": false
        })),
        "fs_write" => Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "allow_override": { "type": "boolean" }
            },
            "required": ["path", "content", "allow_override"],
            "additionalProperties": false
        })),
        "fs_replace" => Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old": { "type": "string" },
                "new": { "type": "string" },
                "mode": { "type": "string", "enum": ["first", "all"] }
            },
            "required": ["path", "old", "new", "mode"],
            "additionalProperties": false
        })),
        "sys_get_context" => Some(json!({
            "type": "object",
            "properties": {
                "include_tools": { "type": "boolean" }
            },
            "required": [],
            "additionalProperties": false
        })),
        "sys_list_profiles" => Some(json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["agent", "user", "all"] }
            },
            "required": ["kind"],
            "additionalProperties": false
        })),
        "sys_get_session_identity_map" => Some(json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        })),
        "sys_get_profile" => Some(json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["agent", "user"] },
                "id": { "type": "string" },
                "view": { "type": "string", "enum": ["summary", "full"] }
            },
            "required": ["kind", "id", "view"],
            "additionalProperties": false
        })),
        "sys_get_task_payload" => Some(json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string" },
                "part": { "type": "string", "enum": ["args", "result"] },
                "offset": { "type": "integer", "minimum": 0 },
                "limit": { "type": "integer", "minimum": 0 }
            },
            "required": ["task_id", "part"],
            "additionalProperties": false
        })),
        _ => None,
    }
}
