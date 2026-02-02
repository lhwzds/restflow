//! Unified tool registry for agent execution.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

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

/// Tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
        }
    }
}

/// Tool definition for LLM function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Trait for executable tools.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get tool definition for LLM.
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with given arguments.
    async fn execute(&self, args: Value) -> Result<ToolResult>;

    /// Tool name (convenience method).
    fn name(&self) -> String {
        self.definition().name
    }
}

/// Central registry for all available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register<T: Tool + 'static>(mut self, tool: T) -> Self {
        let name = tool.definition().name.clone();
        self.tools.insert(name, Arc::new(tool));
        self
    }

    /// Get all tool definitions for LLM.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Execute a tool by name.
    pub async fn execute(&self, name: &str, args: Value) -> Result<ToolResult> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", name))?;
        tool.execute(args).await
    }

    /// Check if a tool exists.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get tool count.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating a fully configured ToolRegistry.
pub struct ToolRegistryBuilder {
    registry: ToolRegistry,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: ToolRegistry::new(),
        }
    }

    /// Add bash tool with security config.
    pub fn with_bash(self, config: BashConfig) -> Self {
        Self {
            registry: self.registry.register(BashTool::new(config)),
        }
    }

    /// Add file tool with allowed paths.
    pub fn with_file(self, config: FileConfig) -> Self {
        Self {
            registry: self.registry.register(FileTool::new(config)),
        }
    }

    /// Add HTTP tool.
    pub fn with_http(self) -> Self {
        Self {
            registry: self.registry.register(HttpTool::new()),
        }
    }

    /// Add Python tool.
    pub fn with_python(self) -> Self {
        Self {
            registry: self.registry.register(PythonTool::new()),
        }
    }

    /// Add email tool.
    pub fn with_email(self) -> Self {
        Self {
            registry: self.registry.register(EmailTool::new()),
        }
    }

    /// Add Telegram tool.
    pub fn with_telegram(self) -> Self {
        Self {
            registry: self.registry.register(TelegramTool::new()),
        }
    }

    /// Add spawn tool for subagent creation.
    pub fn with_spawn(self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        Self {
            registry: self.registry.register(SpawnTool::new(spawner)),
        }
    }

    /// Build the final registry.
    pub fn build(self) -> ToolRegistry {
        self.registry
    }
}


/// Build a tool registry filtered by an allowlist.
///
/// When `tool_names` is `None` or empty, returns an empty registry (secure default).
///
/// Supported aliases:
/// - `python` -> `run_python`
/// - `email` -> `send_email`
/// - `telegram` -> `telegram_send`
/// - `http_request` -> `http`
/// - `read`/`write` -> `file` (write enables file writes)
pub fn registry_from_allowlist(tool_names: Option<&[String]>) -> ToolRegistry {
    let Some(tool_names) = tool_names else {
        return ToolRegistry::new();
    };

    if tool_names.is_empty() {
        return ToolRegistry::new();
    }

    let mut builder = ToolRegistryBuilder::new();
    let mut allow_file = false;
    let mut allow_file_write = false;

    for raw_name in tool_names {
        match raw_name.as_str() {
            "bash" => {
                builder = builder.with_bash(BashConfig::default());
            }
            "file" | "read" => {
                allow_file = true;
            }
            "write" => {
                allow_file = true;
                allow_file_write = true;
            }
            "http" | "http_request" => {
                builder = builder.with_http();
            }
            "run_python" | "python" => {
                builder = builder.with_python();
            }
            "send_email" | "email" => {
                builder = builder.with_email();
            }
            "telegram_send" | "telegram" => {
                builder = builder.with_telegram();
            }
            unknown => {
                warn!(tool_name = %unknown, "Configured tool not found in registry, skipping");
            }
        }
    }

    if allow_file {
        let mut config = FileConfig::default();
        if allow_file_write {
            config.allow_write = true;
        }
        builder = builder.with_file(config);
    }

    builder.build()
}

/// Build the default tool registry with standard tools enabled.
pub fn default_registry() -> ToolRegistry {
    ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .with_http()
        .with_python()
        .with_email()
        .with_telegram()
        .build()
}

impl Default for ToolRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
