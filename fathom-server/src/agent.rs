mod openai;
mod prompt;
mod retry;
mod tool_registry;
mod tools;
mod types;

pub(crate) use tool_registry::ToolRegistry;
pub(crate) use types::{
    AgentTurnOutcome, SessionCompactionSnapshot, SessionIdentityMapSnapshot, StreamNote,
    SummaryBlockRefSnapshot, SystemContextSnapshot, ToolArgDeltaNote, ToolArgDoneNote,
    ToolInvocation, TurnSnapshot,
};

use openai::OpenAiClient;
use prompt::build_tool_only_prompt;

#[derive(Clone)]
pub(crate) struct AgentOrchestrator {
    openai: Option<OpenAiClient>,
    init_error: Option<String>,
    tools: ToolRegistry,
}

impl AgentOrchestrator {
    pub(crate) fn new() -> Self {
        match OpenAiClient::new() {
            Ok(openai) => Self {
                openai: Some(openai),
                init_error: None,
                tools: ToolRegistry::new(),
            },
            Err(error) => Self {
                openai: None,
                init_error: Some(error),
                tools: ToolRegistry::new(),
            },
        }
    }

    pub(crate) async fn run_turn<FS, FT, FD, FN>(
        &self,
        snapshot: &TurnSnapshot,
        mut on_stream: FS,
        mut on_tool: FT,
        mut on_tool_args_delta: FD,
        mut on_tool_args_done: FN,
    ) -> AgentTurnOutcome
    where
        FS: FnMut(StreamNote),
        FT: FnMut(ToolInvocation),
        FD: FnMut(ToolArgDeltaNote),
        FN: FnMut(ToolArgDoneNote),
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
        let mut retry_feedback: Option<&str> = None;

        for semantic_attempt in 0..=1usize {
            on_stream(StreamNote {
                phase: "agent.turn.attempt".to_string(),
                detail: format!("semantic_attempt={}", semantic_attempt + 1),
            });

            let prompt = build_tool_only_prompt(snapshot, retry_feedback);
            let result = openai
                .stream_tool_calls(
                    &prompt,
                    &self.tools,
                    &mut on_stream,
                    |tool_invocation| {
                        on_tool(tool_invocation);
                    },
                    |note| {
                        on_tool_args_delta(note);
                    },
                    |note| {
                        on_tool_args_done(note);
                    },
                )
                .await;

            match result {
                Ok(stream_outcome) if stream_outcome.tool_call_count > 0 => {
                    diagnostics.extend(stream_outcome.diagnostics);
                    diagnostics.push(format!(
                        "tool_calls_dispatched={} on attempt {}",
                        stream_outcome.tool_call_count,
                        semantic_attempt + 1
                    ));
                    return AgentTurnOutcome::success(stream_outcome.tool_call_count, diagnostics);
                }
                Ok(stream_outcome) => {
                    diagnostics.extend(stream_outcome.diagnostics);
                    diagnostics.push(format!(
                        "no tool call generated on attempt {}",
                        semantic_attempt + 1
                    ));

                    if semantic_attempt == 0 {
                        retry_feedback = Some(
                            "No valid executable tool call was produced. You MUST emit at least \
one valid tool call using the provided tool schemas.",
                        );
                        continue;
                    }

                    return AgentTurnOutcome::failure(
                        "no_tool_call",
                        "agent produced no executable tool call after retry",
                        diagnostics,
                    );
                }
                Err(error) => {
                    diagnostics.push(format!("openai request failed: {error}"));
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
