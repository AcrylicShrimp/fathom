use crate::agent::types::{PromptMessageBundle, TurnSnapshot};

use super::prompt::build_agent_prompt_bundle;

#[derive(Debug, Clone, Default)]
pub(crate) struct PromptAssembler;

impl PromptAssembler {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn assemble(
        &self,
        snapshot: &TurnSnapshot,
        retry_feedback: Option<&str>,
    ) -> PromptMessageBundle {
        build_agent_prompt_bundle(snapshot, retry_feedback)
    }
}
