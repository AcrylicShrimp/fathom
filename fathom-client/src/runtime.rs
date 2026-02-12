use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use tonic::transport::Channel;

use crate::pb;
use crate::pb::runtime_service_client::RuntimeServiceClient;
use crate::util::now_unix_ms;

const DEFAULT_AGENT_ID: &str = "agent-default";
const DEFAULT_USER_ID: &str = "user-default";

#[derive(Debug, Clone)]
pub struct ClientSession {
    pub session_id: String,
    pub agent_id: String,
    pub user_id: String,
}

async fn runtime_client(server: &str) -> Result<RuntimeServiceClient<Channel>> {
    let endpoint = Channel::from_shared(server.to_string())?;
    let channel = endpoint.connect().await?;
    Ok(RuntimeServiceClient::new(channel))
}

pub async fn wait_for_server(server: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let result = async {
            let mut client = runtime_client(server).await?;
            client.list_sessions(pb::ListSessionsRequest {}).await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;

        match result {
            Ok(()) => return Ok(()),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
            Err(error) => {
                return Err(anyhow!("server at {server} was not ready in time: {error}"));
            }
        }
    }
}

pub async fn setup_default_session(server: &str) -> Result<ClientSession> {
    let mut client = runtime_client(server).await?;

    let now = now_unix_ms();
    client
        .upsert_agent_profile(pb::UpsertAgentProfileRequest {
            profile: Some(pb::AgentProfile {
                agent_id: DEFAULT_AGENT_ID.to_string(),
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
                user_id: DEFAULT_USER_ID.to_string(),
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
            agent_id: DEFAULT_AGENT_ID.to_string(),
            participant_user_ids: vec![DEFAULT_USER_ID.to_string()],
        })
        .await?
        .into_inner();

    let session_id = create_response
        .session
        .ok_or_else(|| anyhow!("missing session in create_session response"))?
        .session_id;

    Ok(ClientSession {
        session_id,
        agent_id: DEFAULT_AGENT_ID.to_string(),
        user_id: DEFAULT_USER_ID.to_string(),
    })
}

pub async fn attach_session_events(
    server: &str,
    session_id: &str,
) -> Result<tonic::Streaming<pb::SessionEvent>> {
    let mut client = runtime_client(server).await?;
    let stream = client
        .attach_session_events(pb::AttachSessionEventsRequest {
            session_id: session_id.to_string(),
        })
        .await?
        .into_inner();
    Ok(stream)
}

pub async fn enqueue_user_message(
    server: &str,
    session_id: &str,
    user_id: &str,
    text: &str,
) -> Result<String> {
    let mut client = runtime_client(server).await?;
    let response = client
        .enqueue_trigger(pb::EnqueueTriggerRequest {
            session_id: session_id.to_string(),
            trigger: Some(pb::Trigger {
                trigger_id: String::new(),
                created_at_unix_ms: 0,
                kind: Some(pb::trigger::Kind::UserMessage(pb::UserMessageTrigger {
                    user_id: user_id.to_string(),
                    text: text.to_string(),
                })),
            }),
        })
        .await?
        .into_inner();

    Ok(response.trigger_id)
}

pub async fn enqueue_heartbeat(server: &str, session_id: &str) -> Result<String> {
    let mut client = runtime_client(server).await?;
    let response = client
        .enqueue_trigger(pb::EnqueueTriggerRequest {
            session_id: session_id.to_string(),
            trigger: Some(pb::Trigger {
                trigger_id: String::new(),
                created_at_unix_ms: 0,
                kind: Some(pb::trigger::Kind::Heartbeat(pb::HeartbeatTrigger {})),
            }),
        })
        .await?
        .into_inner();

    Ok(response.trigger_id)
}
