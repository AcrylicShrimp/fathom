use std::time::Duration;

use anyhow::{Result, anyhow};
use tonic::transport::Channel;
use tracing::warn;

use crate::pb;
use crate::pb::runtime_service_client::RuntimeServiceClient;
use crate::util::now_unix_ms;
use crate::view::render_event;

async fn runtime_client(server: &str) -> Result<RuntimeServiceClient<Channel>> {
    let endpoint = Channel::from_shared(server.to_string())?;
    let channel = endpoint.connect().await?;
    Ok(RuntimeServiceClient::new(channel))
}

pub async fn bootstrap_demo(server: &str) -> Result<Vec<String>> {
    let mut client = runtime_client(server).await?;

    let now = now_unix_ms();
    client
        .upsert_agent_profile(pb::UpsertAgentProfileRequest {
            profile: Some(pb::AgentProfile {
                agent_id: "agent-default".to_string(),
                display_name: "Fathom".to_string(),
                agents_md: "# AGENTS.md\n".to_string(),
                soul_md: "# SOUL.md\n".to_string(),
                identity_md: "# IDENTITY.md\n".to_string(),
                guidelines_md: "# Guidelines\n".to_string(),
                code_of_conduct_md: "# Code Of Conduct\n".to_string(),
                long_term_memory_md: "# Long-Term Agent Memory\n".to_string(),
                spec_version: 1,
                updated_at_unix_ms: now,
            }),
        })
        .await?;
    client
        .upsert_user_profile(pb::UpsertUserProfileRequest {
            profile: Some(pb::UserProfile {
                user_id: "user-default".to_string(),
                name: "User".to_string(),
                nickname: "user".to_string(),
                preferences_json: "{}".to_string(),
                user_md: "# USER.md\n".to_string(),
                long_term_memory_md: "# Long-Term User Memory\n".to_string(),
                updated_at_unix_ms: now,
            }),
        })
        .await?;

    let create_response = client
        .create_session(pb::CreateSessionRequest {
            agent_id: "agent-default".to_string(),
            participant_user_ids: vec!["user-default".to_string()],
        })
        .await?
        .into_inner();
    let session_id = create_response
        .session
        .ok_or_else(|| anyhow!("missing session"))?
        .session_id;

    let mut events = client
        .attach_session_events(pb::AttachSessionEventsRequest {
            session_id: session_id.clone(),
        })
        .await?
        .into_inner();

    client
        .enqueue_trigger(pb::EnqueueTriggerRequest {
            session_id: session_id.clone(),
            trigger: Some(pb::Trigger {
                trigger_id: String::new(),
                created_at_unix_ms: 0,
                kind: Some(pb::trigger::Kind::UserMessage(pb::UserMessageTrigger {
                    user_id: "user-default".to_string(),
                    text: "/tool memory.append {\"note\":\"remember this\"}".to_string(),
                })),
            }),
        })
        .await?;
    client
        .enqueue_trigger(pb::EnqueueTriggerRequest {
            session_id,
            trigger: Some(pb::Trigger {
                trigger_id: String::new(),
                created_at_unix_ms: 0,
                kind: Some(pb::trigger::Kind::Heartbeat(pb::HeartbeatTrigger {})),
            }),
        })
        .await?;

    let mut lines = Vec::new();
    for _ in 0..8 {
        match tokio::time::timeout(Duration::from_millis(800), events.message()).await {
            Ok(Ok(Some(event))) => lines.push(render_event(&event)),
            Ok(Ok(None)) => break,
            Ok(Err(status)) => {
                lines.push(format!("event stream error: {}", status.message()));
                break;
            }
            Err(_) => break,
        }
    }

    if lines.is_empty() {
        warn!("no events captured in demo bootstrap");
        lines.push("No events captured from server".to_string());
    }
    Ok(lines)
}
