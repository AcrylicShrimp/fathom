mod actor;
mod bootstrap;
mod registry;
mod system;

pub(crate) use actor::{
    CapabilityDomainActionExecution, CapabilityDomainActionSubmission, CapabilityDomainActorHandle,
    CapabilityDomainCommittedAction, CapabilityDomainCommittedExecution,
    spawn_capability_domain_actor,
};
pub(crate) use bootstrap::build_capability_domain_registry;
#[cfg(test)]
pub(crate) use bootstrap::build_default_capability_domain_registry;
pub(crate) use registry::{CapabilityDomainRegistry, ResolvedAction};
#[cfg(test)]
pub(crate) use system::UnavailableSystemInspectionService;
pub(crate) use system::{
    SystemDomainFactory, SystemInspectionError, SystemInspectionFuture, SystemInspectionService,
};
