//! Unified tool registry for agent execution.

use std::sync::Arc;

pub use restflow_ai::tools::{Tool, ToolOutput, ToolRegistry};

mod bash;
mod email;
mod file;
mod http;
mod python;
mod spawn;
mod telegram;

pub use bash::{BashConfig, BashTool};
pub use email::EmailTool;
pub use file::{FileConfig, FileTool};
pub use http::HttpTool;
pub use python::PythonTool;
pub use spawn::{SpawnTool, SubagentSpawner};
pub use telegram::TelegramTool;

pub type ToolResult = ToolOutput;

/// Builder for creating a fully configured ToolRegistry.
pub struct ToolRegistryBuilder {
    registry: ToolRegistry,
}

impl Default for ToolRegistryBuilder {
    fn default() -> Self {
        Self {
            registry: ToolRegistry::new(),
        }
    }
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add bash tool with security config.
    pub fn with_bash(mut self, config: BashConfig) -> Self {
        self.registry.register(BashTool::new(config));
        self
    }

    /// Add file tool with allowed paths.
    pub fn with_file(mut self, config: FileConfig) -> Self {
        self.registry.register(FileTool::new(config));
        self
    }

    /// Add HTTP tool.
    pub fn with_http(mut self) -> Self {
        self.registry.register(HttpTool::new());
        self
    }

    /// Add Python tool.
    pub fn with_python(mut self) -> Self {
        self.registry.register(PythonTool::new());
        self
    }

    /// Add email tool.
    pub fn with_email(mut self) -> Self {
        self.registry.register(EmailTool::new());
        self
    }

    /// Add Telegram tool.
    pub fn with_telegram(mut self) -> Self {
        self.registry.register(TelegramTool::new());
        self
    }

    /// Add spawn tool for subagent creation.
    pub fn with_spawn(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.registry.register(SpawnTool::new(spawner));
        self
    }

    /// Build the final registry.
    pub fn build(self) -> ToolRegistry {
        self.registry
    }
}

/// Build a tool registry filtered by an allowlist.
///
/// When `tool_names` is `None` or empty, returns an empty registry (secure default).
pub fn registry_from_allowlist(tool_names: Option<&[String]>) -> ToolRegistry {
    let allowlist = tool_names.unwrap_or(&[]);
    let mut registry = ToolRegistry::new();

    for tool_name in allowlist {
        match tool_name.as_str() {
            "bash" => registry.register(BashTool::new(BashConfig::default())),
            "file" => registry.register(FileTool::new(FileConfig::default())),
            "http" => registry.register(HttpTool::new()),
            "python" => registry.register(PythonTool::new()),
            "email" => registry.register(EmailTool::new()),
            "telegram" => registry.register(TelegramTool::new()),
            "spawn" => {
                tracing::warn!("spawn tool requires a SubagentSpawner and is skipped");
            }
            _ => {
                tracing::warn!("Unknown tool in allowlist: {}", tool_name);
            }
        }
    }

    registry
}

/// Create a registry with default tools.
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(BashTool::new(BashConfig::default()));
    registry.register(FileTool::new(FileConfig::default()));
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());
    registry.register(TelegramTool::new());
    registry
}
