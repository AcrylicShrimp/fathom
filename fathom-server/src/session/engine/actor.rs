use std::time::Duration;

use tokio::sync::{broadcast, mpsc};

use crate::capability_domain::{CapabilityDomainActorHandle, spawn_capability_domain_actor};
use crate::runtime::Runtime;
use crate::session::state::{SessionCommand, SessionState};
use fathom_protocol::pb;

use super::events::{enqueue_automatic_heartbeat, enqueue_trigger};
use super::tasks::{CommitTurnPolicy, cancel_execution, handle_capability_domain_action_committed};
use super::turn::process_turns;

const AUTO_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub(crate) async fn run_session_actor(
    runtime: Runtime,
    mut state: SessionState,
    command_tx: mpsc::Sender<SessionCommand>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    events_tx: broadcast::Sender<pb::SessionEvent>,
) {
    let capability_domain_handles = state
        .engaged_capability_domain_ids
        .iter()
        .filter_map(|capability_domain_id| {
            state
                .capability_domain_snapshots
                .get(capability_domain_id)
                .cloned()
                .map(|snapshot| {
                    (
                        capability_domain_id.clone(),
                        spawn_capability_domain_actor(
                            runtime.clone(),
                            capability_domain_id.clone(),
                            snapshot,
                            command_tx.clone(),
                        ),
                    )
                })
        })
        .collect::<std::collections::HashMap<String, CapabilityDomainActorHandle>>();

    let mut heartbeat_interval = tokio::time::interval(AUTO_HEARTBEAT_INTERVAL);
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let _ = heartbeat_interval.tick().await;

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };

                match command {
                    SessionCommand::EnqueueTrigger {
                        trigger,
                        respond_to,
                    } => {
                        let queue_depth = enqueue_trigger(&mut state, &events_tx, trigger);
                        let _ = respond_to.send(Ok(pb::EnqueueTriggerResponse {
                            trigger_id: state
                                .trigger_queue
                                .back()
                                .map(|trigger| trigger.trigger_id.clone())
                                .unwrap_or_default(),
                            queue_depth,
                        }));
                        maybe_process_turns(
                            &runtime,
                            &mut state,
                            &command_tx,
                            &events_tx,
                            &capability_domain_handles,
                        )
                        .await;
                    }
                    SessionCommand::GetSummary { respond_to } => {
                        let _ = respond_to.send(state.to_summary());
                    }
                    SessionCommand::ListExecutions { respond_to } => {
                        let mut executions =
                            state.executions.values().cloned().collect::<Vec<_>>();
                        executions.sort_by(|a, b| a.execution_id.cmp(&b.execution_id));
                        let _ = respond_to.send(executions);
                    }
                    SessionCommand::CancelExecution {
                        execution_id,
                        respond_to,
                    } => {
                        let response =
                            cancel_execution(&runtime, &mut state, &events_tx, &execution_id);
                        let _ = respond_to.send(response);
                    }
                    SessionCommand::CapabilityDomainActionCommitted { committed } => {
                        let policy = handle_capability_domain_action_committed(
                            &runtime,
                            &mut state,
                            &events_tx,
                            committed,
                        );
                        if matches!(policy, CommitTurnPolicy::ResumeNow) {
                            maybe_process_turns(
                                &runtime,
                                &mut state,
                                &command_tx,
                                &events_tx,
                                &capability_domain_handles,
                            )
                            .await;
                        }
                    }
                }
            }
            _ = heartbeat_interval.tick() => {
                enqueue_automatic_heartbeat(&runtime, &mut state, &events_tx);
                maybe_process_turns(
                    &runtime,
                    &mut state,
                    &command_tx,
                    &events_tx,
                    &capability_domain_handles,
                )
                .await;
            }
        }
    }
}

async fn maybe_process_turns(
    runtime: &Runtime,
    state: &mut SessionState,
    command_tx: &mpsc::Sender<SessionCommand>,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    capability_domain_handles: &std::collections::HashMap<String, CapabilityDomainActorHandle>,
) {
    if !state.in_flight_actions.is_empty() {
        return;
    }

    process_turns(
        runtime,
        state,
        command_tx,
        events_tx,
        capability_domain_handles,
    )
    .await;
}
