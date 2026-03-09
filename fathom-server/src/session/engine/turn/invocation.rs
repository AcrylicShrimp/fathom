use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::agent::ModelDeltaEvent;
use crate::environment::EnvironmentActorHandle;
use crate::runtime::Runtime;
use crate::session::state::SessionState;
use crate::util::now_unix_ms;
use fathom_protocol::pb;

use super::super::delta_transport::TurnDeltaTransport;
use super::super::events::emit_event;
use super::journal::{
    append_invocation_finished_record, append_invocation_started_record, write_invocation_context,
};
use super::types::{AgentTurnSummary, PreparedTurn};

pub(super) async fn run_agent_invocation(
    runtime: &Runtime,
    state: &mut SessionState,
    events_tx: &broadcast::Sender<pb::SessionEvent>,
    environment_handles: &HashMap<String, EnvironmentActorHandle>,
    turn_id: u64,
    invocation_seq: u64,
    prepared: &mut PreparedTurn,
) -> AgentTurnSummary {
    let assistant_output_start_len = prepared.assistant_outputs.len();
    let context = runtime.build_agent_invocation_context(state, &prepared.agent_triggers);
    let orchestrator = runtime.agent_orchestrator();
    let prompt_bundle = orchestrator.assemble_prompt_bundle(&context, None);

    write_invocation_context(
        runtime,
        state,
        turn_id,
        invocation_seq,
        &context,
        &prompt_bundle,
    );
    append_invocation_started_record(runtime, state, turn_id, invocation_seq);

    let (outcome, stream_notes, action_dispatches, streamed_outputs) = {
        let mut delta_transport =
            TurnDeltaTransport::new(runtime, state, events_tx, environment_handles, turn_id);
        let outcome = orchestrator
            .run_turn(&context, prompt_bundle.clone(), |event: ModelDeltaEvent| {
                delta_transport.handle_model_event(event);
            })
            .await;
        let stream_notes = delta_transport.invocation_stream_notes().to_vec();
        let action_dispatches = delta_transport.action_dispatches().to_vec();
        let streamed_outputs = delta_transport.drain_streamed_assistant_outputs();
        (outcome, stream_notes, action_dispatches, streamed_outputs)
    };

    let action_call_count = outcome.action_call_count;
    let mut model_assistant_outputs = outcome.assistant_outputs;
    let model_diagnostics = outcome.diagnostics;
    let failed = outcome.failed;
    let failure_code = outcome.failure_code;
    let failure_message = outcome.failure_message;

    for (stream_id, output) in streamed_outputs {
        prepared.assistant_outputs.push(output);
        prepared.assistant_stream_ids.push(stream_id);
    }

    for output in model_assistant_outputs.drain(..) {
        if output.trim().is_empty() {
            continue;
        }
        if prepared
            .assistant_outputs
            .last()
            .is_some_and(|last| last == &output)
        {
            continue;
        }
        prepared.assistant_outputs.push(output);
        prepared.assistant_stream_ids.push(String::new());
    }

    for diagnostic in &model_diagnostics {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::AgentStream(pb::AgentStreamEvent {
                phase: "agent.diagnostic".to_string(),
                detail: diagnostic.clone(),
                created_at_unix_ms: now_unix_ms(),
            }),
        );
    }

    if failed {
        emit_event(
            events_tx,
            &state.session_id,
            pb::session_event::Kind::TurnFailure(pb::TurnFailureEvent {
                turn_id,
                reason_code: failure_code.clone(),
                message: failure_message.clone(),
            }),
        );
    }

    let assistant_outputs_slice = prepared
        .assistant_outputs
        .iter()
        .skip(assistant_output_start_len)
        .cloned()
        .collect::<Vec<_>>();
    append_invocation_finished_record(
        runtime,
        state,
        turn_id,
        invocation_seq,
        failed,
        &failure_code,
        &failure_message,
        action_call_count,
        &assistant_outputs_slice,
        &model_diagnostics,
        &stream_notes,
        &action_dispatches,
    );

    AgentTurnSummary {
        action_call_count,
        assistant_output_count: prepared
            .assistant_outputs
            .len()
            .saturating_sub(assistant_output_start_len),
    }
}
