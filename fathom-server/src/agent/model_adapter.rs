use std::future::Future;
use std::pin::Pin;

use crate::agent::{ModelDeltaEvent, ModelInvocationOutcome, PromptMessage, SessionToolCatalog};

pub(crate) type ModelEventSink<'a> = dyn FnMut(ModelDeltaEvent) + Send + 'a;
pub(crate) type ModelAdapterFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelInvocationOutcome, String>> + Send + 'a>>;

pub(crate) trait ModelAdapter: Send + Sync {
    fn provider_name(&self) -> &'static str;

    fn availability_error(&self) -> Option<&str> {
        None
    }

    fn stream_prompt<'a>(
        &'a self,
        prompt_messages: &'a [PromptMessage],
        tool_catalog: &'a SessionToolCatalog,
        on_event: &'a mut ModelEventSink<'a>,
    ) -> ModelAdapterFuture<'a>;
}

pub(crate) struct UnavailableModelAdapter {
    provider_name: &'static str,
    init_error: String,
}

impl UnavailableModelAdapter {
    pub(crate) fn new(provider_name: &'static str, init_error: String) -> Self {
        Self {
            provider_name,
            init_error,
        }
    }
}

impl ModelAdapter for UnavailableModelAdapter {
    fn provider_name(&self) -> &'static str {
        self.provider_name
    }

    fn availability_error(&self) -> Option<&str> {
        Some(&self.init_error)
    }

    fn stream_prompt<'a>(
        &'a self,
        _prompt_messages: &'a [PromptMessage],
        _tool_catalog: &'a SessionToolCatalog,
        _on_event: &'a mut ModelEventSink<'a>,
    ) -> ModelAdapterFuture<'a> {
        let error = self.init_error.clone();
        Box::pin(async move { Err(error) })
    }
}
