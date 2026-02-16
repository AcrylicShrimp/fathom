use anyhow::{Result, anyhow};

use crate::runtime::enqueue_heartbeat;

use super::spec::CommandSpec;

pub(crate) const SPEC: CommandSpec = CommandSpec {
    name: "heartbeat",
    description: "enqueue a heartbeat trigger",
};

pub(crate) async fn execute(server: &str, session_id: &str, args: &str) -> Result<String> {
    if !args.is_empty() {
        return Err(anyhow!("`/heartbeat` does not accept arguments"));
    }

    enqueue_heartbeat(server, session_id).await
}
