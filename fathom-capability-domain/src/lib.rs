mod action;
mod capability_domain;
mod naming;
mod outcome;

pub use action::{Action, ActionModeSupport, ActionSpec};
pub use capability_domain::{
    CapabilityDomain, CapabilityDomainRecipe, CapabilityDomainSnapshot, CapabilityDomainSpec,
    FinalizedAction, TransitionResult,
};
pub use naming::{canonical_action_id, parse_action_id};
pub use outcome::ActionOutcome;
