use std::time::Duration;

use tokio::sync::{broadcast, mpsc};

use crate::pb;
use crate::runtime::Runtime;
use crate::session::state::{SessionCommand, SessionState};

use super::events::{enqueue_automatic_heartbeat, enqueue_trigger};
use super::tasks::{cancel_task, handle_finished_task};
use super::turn::process_turns;

const AUTO_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub(crate) async fn run_session_actor(
    runtime: Runtime,
    mut state: SessionState,
    command_tx: mpsc::Sender<SessionCommand>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    events_tx: broadcast::Sender<pb::SessionEvent>,
) {
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
                        process_turns(&runtime, &mut state, &command_tx, &events_tx).await;
                    }
                    SessionCommand::GetSummary { respond_to } => {
                        let _ = respond_to.send(state.to_summary());
                    }
                    SessionCommand::ListTasks { respond_to } => {
                        let mut tasks = state.tasks.values().cloned().collect::<Vec<_>>();
                        tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
                        let _ = respond_to.send(tasks);
                    }
                    SessionCommand::CancelTask {
                        task_id,
                        respond_to,
                    } => {
                        let response =
                            cancel_task(&runtime, &mut state, &command_tx, &events_tx, &task_id);
                        let _ = respond_to.send(response);
                    }
                    SessionCommand::TaskFinished {
                        task_id,
                        succeeded,
                        message,
                    } => {
                        handle_finished_task(
                            &runtime,
                            &mut state,
                            &command_tx,
                            &events_tx,
                            &task_id,
                            succeeded,
                            message,
                        );
                        process_turns(&runtime, &mut state, &command_tx, &events_tx).await;
                    }
                }
            }
            _ = heartbeat_interval.tick() => {
                enqueue_automatic_heartbeat(&runtime, &mut state, &events_tx);
                process_turns(&runtime, &mut state, &command_tx, &events_tx).await;
            }
        }
    }
}
