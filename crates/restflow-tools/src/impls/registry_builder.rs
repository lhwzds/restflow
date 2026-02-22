//! Tool registry builder with configuration types.
//!
//! Provides BashConfig, FileConfig, and ToolRegistryBuilder for constructing
//! a ToolRegistry with commonly used tools.

use std::path::PathBuf;
use std::sync::Arc;

use crate::registry::ToolRegistry;
use crate::SecretResolver;
use crate::impls::{
    BashTool, DiscordTool, EmailTool, FileTool, HttpTool, SlackTool, TelegramTool,
};
use crate::impls::monty_python::{PythonTool, RunPythonTool};
use crate::impls::transcribe::TranscribeTool;
use crate::impls::vision::VisionTool;
use crate::impls::spawn::SpawnTool;
use crate::impls::spawn_agent::SpawnAgentTool;
use crate::impls::wait_agents::WaitAgentsTool;
use crate::impls::list_agents::ListAgentsTool;
use crate::impls::use_skill::UseSkillTool;
use crate::security::bash_security::BashSecurityConfig;
use restflow_ai::agent::{SubagentDeps, SubagentSpawner};
use restflow_ai::tools::skill_types::SkillProvider;

/// Configuration for bash tool security.
#[derive(Debug, Clone)]
pub struct BashConfig {
    /// Working directory for commands.
    pub working_dir: Option<String>,
    /// Command timeout in seconds.
    pub timeout_secs: u64,
    /// Blocked commands (security).
    pub blocked_commands: Vec<String>,
    /// Whether to allow sudo.
    pub allow_sudo: bool,
    /// Maximum total bytes for stdout/stderr output payload.
    pub max_output_bytes: usize,
}

impl Default for BashConfig {
    fn default() -> Self {
        let security = BashSecurityConfig::default();
        Self {
            working_dir: None,
            timeout_secs: 300,
            blocked_commands: security.blocked_commands,
            allow_sudo: security.allow_sudo,
            max_output_bytes: 1_000_000,
        }
    }
}

impl BashConfig {
    /// Convert into a [`BashTool`].
    pub fn into_bash_tool(self) -> BashTool {
        let mut tool = BashTool::new()
            .with_timeout(self.timeout_secs)
            .with_max_output(self.max_output_bytes);
        if let Some(workdir) = self.working_dir {
            tool = tool.with_workdir(workdir);
        }
        tool
    }
}

/// Configuration for file tool.
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// Allowed paths (security).
    pub allowed_paths: Vec<PathBuf>,
    /// Whether write operations are allowed.
    pub allow_write: bool,
    /// Maximum bytes allowed for a single file read.
    pub max_read_bytes: usize,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/nonexistent")),
            ],
            allow_write: true,
            max_read_bytes: 1_000_000,
        }
    }
}

impl FileConfig {
    /// Convert into a [`FileTool`].
    pub fn into_file_tool(self) -> FileTool {
        let mut tool = FileTool::new().with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        }
        tool
    }
}

/// Builder for creating a fully configured ToolRegistry.
pub struct ToolRegistryBuilder {
    pub registry: ToolRegistry,
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

    pub fn with_bash(mut self, config: BashConfig) -> Self {
        self.registry.register(config.into_bash_tool());
        self
    }

    pub fn with_file(mut self, config: FileConfig) -> Self {
        self.registry.register(config.into_file_tool());
        self
    }

    pub fn with_http(mut self) -> Self {
        self.registry.register(HttpTool::new());
        self
    }

    pub fn with_email(mut self) -> Self {
        self.registry.register(EmailTool::new());
        self
    }

    pub fn with_telegram(mut self) -> Self {
        self.registry.register(TelegramTool::new());
        self
    }

    pub fn with_discord(mut self) -> Self {
        self.registry.register(DiscordTool::new());
        self
    }

    pub fn with_slack(mut self) -> Self {
        self.registry.register(SlackTool::new());
        self
    }

    pub fn with_python(mut self) -> Self {
        self.registry.register(RunPythonTool::new());
        self.registry.register(PythonTool::new());
        self
    }

    pub fn with_transcribe(mut self, resolver: SecretResolver) -> Self {
        self.registry.register(TranscribeTool::new(resolver));
        self
    }

    pub fn with_vision(mut self, resolver: SecretResolver) -> Self {
        self.registry.register(VisionTool::new(resolver));
        self
    }

    pub fn with_spawn(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.registry.register(SpawnTool::new(spawner));
        self
    }

    pub fn with_spawn_agent(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(SpawnAgentTool::new(deps));
        self
    }

    pub fn with_wait_agents(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(WaitAgentsTool::new(deps));
        self
    }

    pub fn with_list_agents(mut self, deps: Arc<SubagentDeps>) -> Self {
        self.registry.register(ListAgentsTool::new(deps));
        self
    }

    pub fn with_use_skill(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(UseSkillTool::new(provider));
        self
    }

    pub fn build(self) -> ToolRegistry {
        self.registry
    }
}

/// Create a registry with default tools.
pub fn default_registry() -> ToolRegistry {
    ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .with_http()
        .with_email()
        .with_telegram()
        .with_discord()
        .with_slack()
        .with_python()
        .build()
}
