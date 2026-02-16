use crate::runtime::ClientSession;

use super::heartbeat;
use super::parse::parse_slash_command;
use super::registry::{CommandId, resolve};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlashExecution {
    NotSlashInput,
    Handled {
        status: String,
        local_log: Option<String>,
    },
}

pub(crate) async fn execute_slash_command(
    input: &str,
    server: &str,
    session: &ClientSession,
) -> SlashExecution {
    let Some(parsed) = parse_slash_command(input) else {
        return SlashExecution::NotSlashInput;
    };

    if parsed.name.is_empty() {
        return local_error("command name is required after `/`");
    }

    let Some(command) = resolve(parsed.name.as_str()) else {
        return local_error(format!("unknown command: /{}", parsed.name));
    };

    match command {
        CommandId::Heartbeat => {
            match heartbeat::execute(server, &session.session_id, &parsed.args).await {
                Ok(trigger_id) => SlashExecution::Handled {
                    status: format!("heartbeat queued ({trigger_id})"),
                    local_log: Some(format!("[local] heartbeat queued id={trigger_id}")),
                },
                Err(error) => local_error(format!("heartbeat failed: {error}")),
            }
        }
    }
}

fn local_error(message: impl Into<String>) -> SlashExecution {
    let message = message.into();
    SlashExecution::Handled {
        status: message.clone(),
        local_log: Some(format!("[local] {message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{SlashExecution, execute_slash_command};
    use crate::runtime::ClientSession;

    fn test_session() -> ClientSession {
        ClientSession {
            session_id: "session-test".to_string(),
            agent_id: "agent-default".to_string(),
            user_id: "user-default".to_string(),
        }
    }

    #[tokio::test]
    async fn not_slash_input_is_not_handled() {
        let execution = execute_slash_command("hello", "http://127.0.0.1:1", &test_session()).await;
        assert_eq!(execution, SlashExecution::NotSlashInput);
    }

    #[tokio::test]
    async fn reports_missing_command_name() {
        let execution = execute_slash_command("/", "http://127.0.0.1:1", &test_session()).await;
        let SlashExecution::Handled { status, local_log } = execution else {
            panic!("expected handled command result");
        };
        assert_eq!(status, "command name is required after `/`");
        assert_eq!(
            local_log.as_deref(),
            Some("[local] command name is required after `/`")
        );
    }

    #[tokio::test]
    async fn reports_unknown_command() {
        let execution = execute_slash_command("/hb", "http://127.0.0.1:1", &test_session()).await;
        let SlashExecution::Handled { status, local_log } = execution else {
            panic!("expected handled command result");
        };
        assert_eq!(status, "unknown command: /hb");
        assert_eq!(local_log.as_deref(), Some("[local] unknown command: /hb"));
    }
}
