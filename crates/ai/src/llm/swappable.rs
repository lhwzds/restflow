//! Swappable LLM wrapper for dynamic model switching

use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::error::Result;
use crate::llm::client::{CompletionRequest, CompletionResponse, LlmClient, StreamResult};

/// LLM wrapper that supports hot-swapping the underlying client.
pub struct SwappableLlm {
    inner: RwLock<Arc<dyn LlmClient>>,
}

impl SwappableLlm {
    /// Create a new swappable LLM wrapper.
    pub fn new(inner: Arc<dyn LlmClient>) -> Self {
        Self {
            inner: RwLock::new(inner),
        }
    }

    /// Swap the underlying LLM client, returning the previous client.
    pub fn swap(&self, new_client: Arc<dyn LlmClient>) -> Arc<dyn LlmClient> {
        let mut guard = self.inner.write();
        std::mem::replace(&mut *guard, new_client)
    }

    /// Get the current provider name.
    pub fn current_provider(&self) -> String {
        let guard = self.inner.read();
        guard.provider().to_string()
    }

    /// Get the current model name.
    pub fn current_model(&self) -> String {
        let guard = self.inner.read();
        guard.model().to_string()
    }
}

#[async_trait]
impl LlmClient for SwappableLlm {
    fn provider(&self) -> &str {
        "swappable"
    }

    fn model(&self) -> &str {
        "dynamic"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let client = {
            let guard = self.inner.read();
            guard.clone()
        };
        client.complete(request).await
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let client = {
            let guard = self.inner.read();
            guard.clone()
        };
        client.complete_stream(request)
    }
}
