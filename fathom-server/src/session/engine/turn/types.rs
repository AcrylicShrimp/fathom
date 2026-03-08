use crate::pb;

#[derive(Debug, Clone, Copy)]
pub(super) struct AgentTurnSummary {
    pub(super) action_call_count: usize,
    pub(super) assistant_output_count: usize,
}

#[derive(Debug)]
pub(super) struct PreparedTurn {
    pub(super) turn_triggers: Vec<pb::Trigger>,
    pub(super) agent_triggers: Vec<pb::Trigger>,
    pub(super) assistant_outputs: Vec<String>,
    pub(super) assistant_stream_ids: Vec<String>,
}

impl PreparedTurn {
    pub(super) fn new(turn_triggers: Vec<pb::Trigger>) -> Self {
        Self {
            turn_triggers,
            agent_triggers: Vec::new(),
            assistant_outputs: Vec::new(),
            assistant_stream_ids: Vec::new(),
        }
    }
}
