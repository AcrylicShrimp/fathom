use serde_json::{Map, Value, json};

use fathom_protocol::pb;

pub(crate) fn default_agent_material_json(agent_id: &str) -> String {
    json!({
        "identity": {
            "agent_id": agent_id,
            "mission": "Help the user directly and choose the next useful action when needed."
        },
        "behavior": {
            "style": "pragmatic, clear, direct",
            "guidelines": [
                "Prefer deterministic behavior.",
                "Do not take harmful actions."
            ]
        },
        "memory": {
            "long_term": ""
        }
    })
    .to_string()
}

pub(crate) fn default_user_material_json(user_id: &str) -> String {
    json!({
        "identity": {
            "user_id": user_id
        },
        "preferences": {},
        "memory": {
            "long_term": ""
        }
    })
    .to_string()
}

pub(crate) fn agent_identity_material(profile: &pb::AgentProfile) -> Value {
    let mut material = parse_material_object(&profile.material_json);
    material
        .entry("display_name".to_string())
        .or_insert_with(|| Value::String(profile.display_name.clone()));
    Value::Object(material)
}

pub(crate) fn participant_profile_material(profile: &pb::UserProfile) -> Value {
    let mut material = parse_material_object(&profile.material_json);
    material
        .entry("user_id".to_string())
        .or_insert_with(|| Value::String(profile.user_id.clone()));
    material
        .entry("name".to_string())
        .or_insert_with(|| Value::String(profile.name.clone()));
    material
        .entry("nickname".to_string())
        .or_insert_with(|| Value::String(profile.nickname.clone()));
    Value::Object(material)
}

pub(crate) fn validate_material_json_object(material_json: &str) -> Result<(), String> {
    match serde_json::from_str::<Value>(material_json) {
        Ok(Value::Object(_)) => Ok(()),
        Ok(_) => Err("material_json must be a JSON object".to_string()),
        Err(error) => Err(format!("material_json must be valid JSON: {error}")),
    }
}

fn parse_material_object(material_json: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(material_json) {
        Ok(Value::Object(map)) => map,
        Ok(other) => Map::from_iter([("value".to_string(), other)]),
        Err(error) => Map::from_iter([(
            "invalid_material_json".to_string(),
            Value::String(error.to_string()),
        )]),
    }
}
