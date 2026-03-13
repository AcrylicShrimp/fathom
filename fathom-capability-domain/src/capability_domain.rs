use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::{CapabilityActionDefinition, CapabilityActionResult, CapabilityActionSubmission};

#[derive(Debug, Clone)]
pub struct CapabilityDomainSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDomainRecipe {
    pub title: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CapabilityDomainSessionContext {
    pub session_id: String,
}

pub type DomainInstanceFuture<'a> =
    Pin<Box<dyn Future<Output = Vec<CapabilityActionResult>> + Send + 'a>>;

pub trait DomainFactory: Send + Sync + 'static {
    fn spec(&self) -> CapabilityDomainSpec;

    fn recipes(&self) -> Vec<CapabilityDomainRecipe> {
        Vec::new()
    }

    fn actions(&self) -> Vec<CapabilityActionDefinition>;

    fn create_instance(
        &self,
        session_context: CapabilityDomainSessionContext,
    ) -> Box<dyn DomainInstance>;
}

pub trait DomainInstance: Send {
    fn execute_actions<'a>(
        &'a mut self,
        submissions: Vec<CapabilityActionSubmission>,
    ) -> DomainInstanceFuture<'a>;
}
