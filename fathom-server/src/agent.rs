mod openai;
mod prompt;
mod retry;
mod types;

pub(crate) use types::{
    ActionArgDeltaNote, ActionArgDoneNote, ActionInvocation, ActivatedEnvironmentActionHint,
    ActivatedEnvironmentHint, ActivatedEnvironmentRecipeHint, AgentTurnOutcome, InFlightActionHint,
    ResolvedPayloadLookupHint, SessionCompactionSnapshot, SessionIdentityMapSnapshot, StreamNote,
    SummaryBlockRefSnapshot, SystemContextSnapshot, SystemTimeContext, TurnSnapshot,
};

use crate::environment::EnvironmentRegistry;
use openai::OpenAiClient;
use prompt::build_agent_prompt;

pub(crate) fn render_prompt(snapshot: &TurnSnapshot, retry_feedback: Option<&str>) -> String {
    build_agent_prompt(snapshot, retry_feedback)
}

#[derive(Clone)]
pub(crate) struct AgentOrchestrator {
    openai: Option<OpenAiClient>,
    init_error: Option<String>,
    environment_registry: EnvironmentRegistry,
}

impl AgentOrchestrator {
    pub(crate) fn new() -> Self {
        match OpenAiClient::new() {
            Ok(openai) => Self {
                openai: Some(openai),
                init_error: None,
                environment_registry: EnvironmentRegistry::new(),
            },
            Err(error) => Self {
                openai: None,
                init_error: Some(error),
                environment_registry: EnvironmentRegistry::new(),
            },
        }
    }

    pub(crate) async fn run_turn<FS, FA, FD, FN, FT, FC>(
        &self,
        snapshot: &TurnSnapshot,
        mut on_stream: FS,
        mut on_action: FA,
        mut on_action_args_delta: FD,
        mut on_action_args_done: FN,
        mut on_assistant_delta: FT,
        mut on_assistant_done: FC,
    ) -> AgentTurnOutcome
    where
        FS: FnMut(StreamNote),
        FA: FnMut(ActionInvocation),
        FD: FnMut(ActionArgDeltaNote),
        FN: FnMut(ActionArgDoneNote),
        FT: FnMut(String),
        FC: FnMut(String),
    {
        if let Some(error) = &self.init_error {
            return AgentTurnOutcome::failure(
                "agent_init_error",
                format!("agent initialization failed: {error}"),
                Vec::new(),
            );
        }

        let Some(openai) = self.openai.as_ref() else {
            return AgentTurnOutcome::failure(
                "agent_init_error",
                "agent initialization failed: OpenAI client is unavailable",
                Vec::new(),
            );
        };

        let mut diagnostics = Vec::new();
        let mut retry_feedback: Option<String> = None;

        for semantic_attempt in 0..=1usize {
            on_stream(StreamNote {
                phase: "agent.turn.attempt".to_string(),
                detail: format!("semantic_attempt={}", semantic_attempt + 1),
            });

            let prompt = build_agent_prompt(snapshot, retry_feedback.as_deref());
            let result = openai
                .stream_actions(
                    &prompt,
                    &self.environment_registry,
                    &mut on_stream,
                    |action_invocation| {
                        on_action(action_invocation);
                    },
                    |note| {
                        on_action_args_delta(note);
                    },
                    |note| {
                        on_action_args_done(note);
                    },
                    |delta| {
                        on_assistant_delta(delta);
                    },
                    |text| {
                        on_assistant_done(text);
                    },
                )
                .await;

            match result {
                Ok(stream_outcome)
                    if stream_outcome.action_call_count > 0
                        || !stream_outcome.assistant_outputs.is_empty() =>
                {
                    diagnostics.extend(stream_outcome.diagnostics);
                    diagnostics.push(format!(
                        "action_calls_dispatched={} assistant_outputs={} on attempt {}",
                        stream_outcome.action_call_count,
                        stream_outcome.assistant_outputs.len(),
                        semantic_attempt + 1
                    ));
                    return AgentTurnOutcome::success(
                        stream_outcome.action_call_count,
                        stream_outcome.assistant_outputs,
                        diagnostics,
                    );
                }
                Ok(stream_outcome) => {
                    diagnostics.extend(stream_outcome.diagnostics);
                    diagnostics.push(format!(
                        "no action call or assistant output generated on attempt {}",
                        semantic_attempt + 1
                    ));

                    if semantic_attempt == 0 {
                        retry_feedback = Some(
                            "No valid executable action call or assistant output was produced. \
You MUST emit at least one valid action call or assistant output."
                                .to_string(),
                        );
                        continue;
                    }

                    return AgentTurnOutcome::failure(
                        "no_action_or_output",
                        "agent produced no executable action call or assistant output after retry",
                        diagnostics,
                    );
                }
                Err(error) => {
                    diagnostics.push(format!("openai request failed: {error}"));
                    if semantic_attempt == 0 && is_recoverable_action_error(&error) {
                        retry_feedback = Some(format!(
                            "The previous action call was invalid and could not be executed: {error}\n\
Emit a corrected action call with valid arguments, or emit assistant output."
                        ));
                        diagnostics.push(
                            "retrying semantic attempt due to recoverable action-call error"
                                .to_string(),
                        );
                        continue;
                    }
                    return AgentTurnOutcome::failure("openai_error", error, diagnostics);
                }
            }
        }

        AgentTurnOutcome::failure(
            "agent_unreachable",
            "unexpected agent loop termination",
            diagnostics,
        )
    }
}

fn is_recoverable_action_error(error: &str) -> bool {
    error.contains("validation failed")
        || error.contains("invalid arguments JSON for action")
        || error.contains("unknown action `")
}
