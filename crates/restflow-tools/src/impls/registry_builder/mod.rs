//! Tool registry builder with configuration types.
//!
//! Provides BashConfig, FileConfig, and ToolRegistryBuilder for constructing
//! a ToolRegistry with commonly used tools.

mod base_tools;
pub(crate) mod configs;
mod runtime_tools;
mod storage_tools;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::ToolRegistry;
use crate::impls::batch::BatchTool;
use crate::impls::file_tracker::FileTracker;

pub use self::configs::{BashConfig, FileConfig, SecretsConfig};

/// Builder for creating a fully configured ToolRegistry.
pub struct ToolRegistryBuilder {
    pub registry: ToolRegistry,
    tracker: Arc<FileTracker>,
}

impl Default for ToolRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: ToolRegistry::new(),
            tracker: Arc::new(FileTracker::new()),
        }
    }

    /// Get shared file tracker for external use.
    pub fn tracker(&self) -> Arc<FileTracker> {
        self.tracker.clone()
    }

    pub fn build(self) -> ToolRegistry {
        self.registry
    }

    /// Build the registry and automatically register the `batch` tool.
    ///
    /// This is a convenience for the two-phase setup required by `BatchTool`,
    /// which needs an `Arc<ToolRegistry>` containing the base tools it can call.
    pub fn build_with_batch(self) -> ToolRegistry {
        let mut registry = self.build();
        if registry.has("batch") {
            return registry;
        }

        let registry_arc = Arc::new(std::mem::take(&mut registry));
        for name in registry_arc.list() {
            if let Some(tool) = registry_arc.get(name) {
                registry.register_arc(tool);
            }
        }
        registry.register(BatchTool::new(registry_arc));
        registry
    }
}

/// Create a registry with default tools.
pub fn default_registry() -> std::result::Result<ToolRegistry, reqwest::Error> {
    Ok(ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .with_http()?
        .with_email()
        .with_telegram()?
        .with_discord()?
        .with_slack()?
        .with_python()
        .build())
}
