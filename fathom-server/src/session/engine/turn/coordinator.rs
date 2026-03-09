use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::environment::EnvironmentActorHandle;
use crate::runtime::Runtime;
use crate::session::state::SessionState;
use fathom_protocol::pb;

use super::super::events::emit_event;
use super::super::history_flush::flush_history;
use super::super::profiles::apply_profile_refresh;
use super::invocation::run_agent_invocation;
use super::journal::{append_turn_ended_record, append_turn_started_record};
use super::types::{AgentTurnSummary, PreparedTurn};

pub(super) struct TurnCoordinator<'a> {
    runtime: &'a Runtime,
    state: &'a mut SessionState,
    events_tx: &'a broadcast::Sender<pb::SessionEvent>,
    environment_handles: &'a HashMap<String, EnvironmentActorHandle>,
}

impl<'a> TurnCoordinator<'a> {
    pub(super) fn new(
        runtime: &'a Runtime,
        state: &'a mut SessionState,
        events_tx: &'a broadcast::Sender<pb::SessionEvent>,
        environment_handles: &'a HashMap<String, EnvironmentActorHandle>,
    ) -> Self {
        Self {
            runtime,
            state,
            events_tx,
            environment_handles,
        }
    }

    pub(super) async fn process(&mut self) {
        if self.is_blocked() {
            return;
        }

        self.state.turn_in_progress = true;
        while !self.state.trigger_queue.is_empty() && self.state.in_flight_actions.is_empty() {
            let turn_id = self.allocate_turn_id();
            let turn_triggers = self.drain_turn_triggers();

            append_turn_started_record(self.runtime, self.state, turn_id, &turn_triggers);
            self.emit_turn_started(turn_id, turn_triggers.len());

            let mut prepared = PreparedTurn::new(turn_triggers);
            self.preprocess_triggers(&mut prepared).await;

            let agent_summary = if prepared.agent_triggers.is_empty() {
                None
            } else {
                let invocation_seq = self.state.allocate_agent_invocation_seq();
                Some(
                    run_agent_invocation(
                        self.runtime,
                        self.state,
                        self.events_tx,
                        self.environment_handles,
                        turn_id,
                        invocation_seq,
                        &mut prepared,
                    )
                    .await,
                )
            };

            self.finalize_turn(turn_id, prepared, agent_summary);
        }
        self.state.turn_in_progress = false;
    }

    fn is_blocked(&self) -> bool {
        self.state.turn_in_progress
            || self.state.trigger_queue.is_empty()
            || !self.state.in_flight_actions.is_empty()
    }

    fn allocate_turn_id(&mut self) -> u64 {
        self.state.turn_seq += 1;
        self.state.turn_seq
    }

    fn drain_turn_triggers(&mut self) -> Vec<pb::Trigger> {
        let mut turn_triggers = Vec::with_capacity(self.state.trigger_queue.len());
        while let Some(trigger) = self.state.trigger_queue.pop_front() {
            turn_triggers.push(trigger);
        }
        turn_triggers
    }

    async fn preprocess_triggers(&mut self, prepared: &mut PreparedTurn) {
        for trigger in &prepared.turn_triggers {
            match trigger.kind.as_ref() {
                Some(pb::trigger::Kind::RefreshProfile(refresh)) => {
                    let refreshed_user_ids =
                        apply_profile_refresh(self.runtime, self.state, refresh).await;
                    emit_event(
                        self.events_tx,
                        &self.state.session_id,
                        pb::session_event::Kind::ProfileRefreshed(pb::ProfileRefreshedEvent {
                            scope: refresh.scope,
                            refreshed_user_ids,
                            agent_spec_version: self.state.agent_profile_copy.spec_version,
                        }),
                    );
                    emit_event(
                        self.events_tx,
                        &self.state.session_id,
                        pb::session_event::Kind::SystemNotice(pb::SystemNoticeEvent {
                            level: pb::SystemNoticeLevel::Info as i32,
                            code: "profile_refresh".to_string(),
                            message: "profile copies refreshed for this session".to_string(),
                        }),
                    );
                }
                _ => prepared.agent_triggers.push(trigger.clone()),
            }
        }
    }

    fn finalize_turn(
        &mut self,
        turn_id: u64,
        prepared: PreparedTurn,
        agent_summary: Option<AgentTurnSummary>,
    ) {
        for (index, output) in prepared.assistant_outputs.iter().enumerate() {
            let stream_id = prepared
                .assistant_stream_ids
                .get(index)
                .cloned()
                .unwrap_or_default();
            emit_event(
                self.events_tx,
                &self.state.session_id,
                pb::session_event::Kind::AssistantOutput(pb::AssistantOutputEvent {
                    content: output.clone(),
                    stream_id,
                }),
            );
        }

        flush_history(
            self.state,
            &prepared.turn_triggers,
            &prepared.assistant_outputs,
        );
        let reason = format!("processed {} trigger(s)", prepared.turn_triggers.len());
        emit_event(
            self.events_tx,
            &self.state.session_id,
            pb::session_event::Kind::TurnEnded(pb::TurnEndedEvent {
                turn_id,
                reason,
                history_size: self.state.history.len() as u64,
            }),
        );

        let is_quiescent = agent_summary.is_some_and(|summary| {
            summary.assistant_output_count > 0
                && summary.action_call_count == 0
                && self.state.in_flight_actions.is_empty()
                && self.state.trigger_queue.is_empty()
        });
        if is_quiescent {
            self.state.pending_payload_lookups.clear();
        }
        append_turn_ended_record(
            self.runtime,
            self.state,
            turn_id,
            agent_summary,
            is_quiescent,
        );
    }

    fn emit_turn_started(&self, turn_id: u64, trigger_count: usize) {
        emit_event(
            self.events_tx,
            &self.state.session_id,
            pb::session_event::Kind::TurnStarted(pb::TurnStartedEvent {
                turn_id,
                trigger_count: trigger_count as u64,
            }),
        );
    }
}
