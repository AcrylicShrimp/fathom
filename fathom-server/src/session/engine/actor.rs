use std::future::pending;
use std::time::Duration;

use fathom_capability_domain::CapabilityDomainSessionContext;
use tokio::sync::{broadcast, mpsc};

use crate::capability_domain::{CapabilityDomainActorHandle, spawn_capability_domain_actor};
use crate::runtime::Runtime;
use crate::session::inspection;
use crate::session::state::{SessionCommand, SessionState};
use fathom_protocol::pb;

use super::events::{enqueue_automatic_heartbeat, enqueue_trigger};
use super::tasks::{
    background_expired_submissions, cancel_execution, handle_capability_domain_action_committed,
};
use super::turn::process_turns;

const AUTO_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub(crate) async fn run_session_actor(
    runtime: Runtime,
    mut state: SessionState,
    command_tx: mpsc::Sender<SessionCommand>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    events_tx: broadcast::Sender<pb::SessionEvent>,
) {
    let registry = runtime.capability_domain_registry();
    let capability_domain_handles = state
        .engaged_capability_domain_ids
        .iter()
        .filter_map(|capability_domain_id| {
            registry
                .domain_factory(capability_domain_id)
                .map(|domain_factory| {
                    (
                        capability_domain_id.clone(),
                        spawn_capability_domain_actor(
                            capability_domain_id.clone(),
                            domain_factory.create_instance(CapabilityDomainSessionContext {
                                session_id: state.session_id.clone(),
                            }),
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
        let foreground_wait_deadline = state.next_foreground_wait_deadline();
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
                    SessionCommand::InspectListExecutions { query, respond_to } => {
                        let _ = respond_to.send(inspection::list_executions(&state, &query));
                    }
                    SessionCommand::InspectGetExecution {
                        execution_id,
                        respond_to,
                    } => {
                        let _ = respond_to.send(Ok(inspection::get_execution(&state, &execution_id)));
                    }
                    SessionCommand::InspectReadExecutionInput {
                        execution_id,
                        offset,
                        limit,
                        respond_to,
                    } => {
                        let _ = respond_to.send(inspection::read_execution_input(
                            &state,
                            &execution_id,
                            offset,
                            limit,
                        ));
                    }
                    SessionCommand::InspectReadExecutionResult {
                        execution_id,
                        offset,
                        limit,
                        respond_to,
                    } => {
                        let _ = respond_to.send(inspection::read_execution_result(
                            &state,
                            &execution_id,
                            offset,
                            limit,
                        ));
                    }
                    SessionCommand::CancelExecution {
                        execution_id,
                        respond_to,
                    } => {
                        let response =
                            cancel_execution(
                                &runtime,
                                &mut state,
                                &events_tx,
                                &capability_domain_handles,
                                &execution_id,
                            );
                        let _ = respond_to.send(response);
                    }
                    SessionCommand::CapabilityDomainActionCommitted { committed } => {
                        handle_capability_domain_action_committed(
                            &runtime,
                            &mut state,
                            &events_tx,
                            &capability_domain_handles,
                            committed,
                        );
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
            _ = async {
                if let Some(deadline) = foreground_wait_deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    pending::<()>().await;
                }
            } => {
                if background_expired_submissions(&runtime, &mut state, &events_tx) {
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
    if state.has_blocking_submissions() {
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
