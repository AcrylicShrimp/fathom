use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub(crate) struct ToolSpec {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) parameters: Value,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ToolRegistry {
    tools: Vec<ToolSpec>,
}

impl ToolRegistry {
    pub(crate) fn new() -> Self {
        Self {
            tools: vec![
                ToolSpec {
                    name: "memory_append",
                    description: "Append a durable note to agent or user long-term memory.",
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "target": { "type": "string", "enum": ["agent", "user"] },
                            "target_id": { "type": "string" },
                            "note": { "type": "string" }
                        },
                        "required": ["target", "target_id", "note"],
                        "additionalProperties": false
                    }),
                },
                ToolSpec {
                    name: "refresh_profile",
                    description: "Refresh the session-local immutable profile copy for agent/user/all.",
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "scope": { "type": "string", "enum": ["agent", "user", "all"] },
                            "user_id": { "type": "string" }
                        },
                        "required": ["scope"],
                        "additionalProperties": false
                    }),
                },
                ToolSpec {
                    name: "schedule_heartbeat",
                    description: "Schedule a heartbeat-style background job for the current session.",
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "delay_ms": { "type": "integer", "minimum": 0 }
                        },
                        "required": ["delay_ms"],
                        "additionalProperties": false
                    }),
                },
            ],
        }
    }

    pub(crate) fn openai_tool_definitions(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                    "strict": true
                })
            })
            .collect()
    }

    pub(crate) fn validate(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        let args_obj = args
            .as_object()
            .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;

        match tool_name {
            "memory_append" => {
                let target = read_required_string(args_obj, "target")?;
                if target != "agent" && target != "user" {
                    return Err("memory_append.target must be 'agent' or 'user'".to_string());
                }
                read_required_string(args_obj, "target_id")?;
                read_required_string(args_obj, "note")?;
                Ok(())
            }
            "refresh_profile" => {
                let scope = read_required_string(args_obj, "scope")?;
                if scope != "agent" && scope != "user" && scope != "all" {
                    return Err(
                        "refresh_profile.scope must be 'agent', 'user', or 'all'".to_string()
                    );
                }
                if scope == "user" {
                    read_required_string(args_obj, "user_id")?;
                }
                Ok(())
            }
            "schedule_heartbeat" => {
                let delay = args_obj
                    .get("delay_ms")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| "schedule_heartbeat.delay_ms must be an integer".to_string())?;
                if delay < 0 {
                    return Err("schedule_heartbeat.delay_ms must be >= 0".to_string());
                }
                Ok(())
            }
            _ => Err(format!("unknown tool `{tool_name}`")),
        }
    }
}

fn read_required_string(
    args: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing or invalid string field `{key}`"))
}
