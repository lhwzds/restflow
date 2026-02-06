//! Unified tool registry for agent execution.

use std::sync::Arc;
use tracing::warn;

use crate::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};
use restflow_ai::LlmClient;

pub use restflow_ai::tools::{
    SecretResolver, Tool, ToolOutput, ToolRegistry, TranscribeTool, VisionTool,
};

mod bash;
mod email;
mod file;
mod http;
mod list_agents;
mod python;
mod spawn;
mod spawn_agent;
mod telegram;
mod use_skill;
mod wait_agents;

pub use bash::{BashConfig, BashTool};
pub use email::EmailTool;
pub use file::{FileConfig, FileTool};
pub use http::HttpTool;
pub use list_agents::ListAgentsTool;
pub use python::PythonTool;
pub use spawn::{SpawnTool, SubagentSpawner};
pub use spawn_agent::SpawnAgentTool;
pub use telegram::TelegramTool;
pub use use_skill::UseSkillTool;
pub use wait_agents::WaitAgentsTool;

pub type ToolResult = ToolOutput;

/// Dependencies needed for advanced sub-agent tools.
#[derive(Clone)]
pub struct SubagentDeps {
    pub tracker: Arc<SubagentTracker>,
    pub definitions: Arc<AgentDefinitionRegistry>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub config: SubagentConfig,
}

/// Builder for creating a fully configured ToolRegistry.
pub struct ToolRegistryBuilder {
    registry: ToolRegistry,
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
        }
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

    /// Add transcribe tool.
    pub fn with_transcribe(mut self, resolver: SecretResolver) -> Self {
        self.registry.register(TranscribeTool::new(resolver));
        self
    }

    /// Add vision tool.
    pub fn with_vision(mut self, resolver: SecretResolver) -> Self {
        self.registry.register(VisionTool::new(resolver));
        self
    }

    /// Add spawn tool for subagent creation.
    pub fn with_spawn(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.registry.register(SpawnTool::new(spawner));
        self
    }

    /// Add spawn_agent tool for sub-agent management.
    pub fn with_spawn_agent(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(SpawnAgentTool::new(deps));
        self
    }

    /// Add wait_agents tool for sub-agent management.
    pub fn with_wait_agents(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(WaitAgentsTool::new(deps));
        self
    }

    /// Add list_agents tool for sub-agent management.
    pub fn with_list_agents(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(ListAgentsTool::new(deps));
        self
    }

    /// Add use_skill tool for sub-agent management.
    pub fn with_use_skill(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(UseSkillTool::new(deps));
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
///
/// Supported aliases:
/// - `python` -> `run_python`
/// - `email` -> `send_email`
/// - `telegram` -> `telegram_send`
/// - `http_request` -> `http`
/// - `read`/`write` -> `file` (write enables file writes)
pub fn registry_from_allowlist(
    tool_names: Option<&[String]>,
    subagent_deps: Option<&SubagentDeps>,
    secret_resolver: Option<SecretResolver>,
) -> ToolRegistry {
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
            "transcribe" => {
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_transcribe(resolver);
                } else {
                    warn!(
                        tool_name = "transcribe",
                        "Secret resolver missing, skipping"
                    );
                }
            }
            "vision" => {
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_vision(resolver);
                } else {
                    warn!(tool_name = "vision", "Secret resolver missing, skipping");
                }
            }
            "spawn_agent" => {
                if let Some(deps) = subagent_deps {
                    builder = builder.with_spawn_agent(Arc::new(deps.clone()));
                } else {
                    warn!(
                        tool_name = "spawn_agent",
                        "Subagent dependencies missing, skipping"
                    );
                }
            }
            "wait_agents" => {
                if let Some(deps) = subagent_deps {
                    builder = builder.with_wait_agents(Arc::new(deps.clone()));
                } else {
                    warn!(
                        tool_name = "wait_agents",
                        "Subagent dependencies missing, skipping"
                    );
                }
            }
            "list_agents" => {
                if let Some(deps) = subagent_deps {
                    builder = builder.with_list_agents(Arc::new(deps.clone()));
                } else {
                    warn!(
                        tool_name = "list_agents",
                        "Subagent dependencies missing, skipping"
                    );
                }
            }
            "use_skill" => {
                if let Some(deps) = subagent_deps {
                    builder = builder.with_use_skill(Arc::new(deps.clone()));
                } else {
                    warn!(
                        tool_name = "use_skill",
                        "Subagent dependencies missing, skipping"
                    );
                }
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

/// Create a registry with default tools.
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
