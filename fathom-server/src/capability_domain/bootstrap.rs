use std::path::Path;
use std::sync::Arc;

use fathom_capability_domain::DomainFactory;

use super::registry::CapabilityDomainRegistry;
use super::{SystemDomainFactory, SystemInspectionService};

#[cfg(test)]
use super::UnavailableSystemInspectionService;

#[cfg(test)]
pub(crate) fn build_default_capability_domain_registry(
    workspace_root: &Path,
) -> CapabilityDomainRegistry {
    build_capability_domain_registry(workspace_root, Arc::new(UnavailableSystemInspectionService))
}

pub(crate) fn build_capability_domain_registry(
    workspace_root: &Path,
    system_inspection_service: Arc<dyn SystemInspectionService>,
) -> CapabilityDomainRegistry {
    CapabilityDomainRegistry::from_domain_factories(default_domain_factories(
        workspace_root,
        system_inspection_service,
    ))
}

fn default_domain_factories(
    workspace_root: &Path,
    system_inspection_service: Arc<dyn SystemInspectionService>,
) -> Vec<Arc<dyn DomainFactory>> {
    vec![
        Arc::new(fathom_capability_domain_fs::FilesystemDomainFactory::new(
            workspace_root.to_path_buf(),
        )),
        Arc::new(fathom_capability_domain_brave_search::BraveSearchDomainFactory::new()),
        Arc::new(fathom_capability_domain_jina::JinaDomainFactory::new()),
        Arc::new(fathom_capability_domain_shell::ShellDomainFactory::new(
            workspace_root.to_path_buf(),
        )),
        Arc::new(SystemDomainFactory::new(system_inspection_service)),
    ]
}
