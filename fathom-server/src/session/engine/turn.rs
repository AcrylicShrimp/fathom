mod coordinator;
mod invocation;
mod journal;
mod types;

use std::collections::HashMap;

use tokio::sync::{broadcast, mpsc};

use crate::capability_domain::CapabilityDomainActorHandle;
use crate::runtime::Runtime;
use crate::session::state::{SessionCommand, SessionState};
use fathom_protocol::pb;

use self::coordinator::TurnCoordinator;

pub(super) async fn process_turns(
    runtime: &Runtime,
    state: &mut SessionState,
    _command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &HashMap<String, CapabilityDomainActorHandle>,
) {
    TurnCoordinator::new(runtime, state, events_tx, capability_domain_handles)
        .process()
        .await;
}
