//! Unified tool registry for agent execution.

use std::sync::Arc;
use tracing::warn;

use crate::runtime::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};
use crate::services::tool_registry::create_tool_registry;
use crate::storage::Storage;
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

pub fn secret_resolver_from_storage(storage: &Storage) -> SecretResolver {
    let secrets = storage.secrets.clone();
    Arc::new(move |key| secrets.get_secret(key).ok().flatten())
}

/// Default tools for main agents.
///
/// These defaults are shared by channel chat dispatch and workspace session chat
/// so both execution paths expose a consistent baseline capability set.
pub fn main_agent_default_tool_names() -> Vec<String> {
    vec![
        "bash",
        "file",
        "http",
        "python",
        "email",
        "telegram",
        "transcribe",
        "vision",
        "spawn_agent",
        "wait_agents",
        "list_agents",
        "use_skill",
        "manage_tasks",
        "manage_agents",
        "manage_marketplace",
        "manage_triggers",
        "manage_terminal",
        "security_query",
        "switch_model",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Merge the default main-agent tools with agent-specific additions.
pub fn effective_main_agent_tool_names(tool_names: Option<&[String]>) -> Vec<String> {
    let mut merged = main_agent_default_tool_names();
    if let Some(extra) = tool_names {
        for name in extra {
            if !merged.iter().any(|item| item == name) {
                merged.push(name.clone());
            }
        }
    }
    merged
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

    /// Add transcription tool.
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
    storage: Option<&Storage>,
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
    let mut enable_manage_tasks = false;
    let mut enable_manage_agents = false;
    let mut enable_manage_marketplace = false;
    let mut enable_manage_triggers = false;
    let mut enable_manage_terminal = false;
    let mut enable_security_query = false;

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
            "manage_tasks" => {
                enable_manage_tasks = true;
            }
            "manage_agents" => {
                enable_manage_agents = true;
            }
            "manage_marketplace" => {
                enable_manage_marketplace = true;
            }
            "manage_triggers" => {
                enable_manage_triggers = true;
            }
            "manage_terminal" => {
                enable_manage_terminal = true;
            }
            "security_query" => {
                enable_security_query = true;
            }
            "switch_model" => {
                // Registered by callers that provide SwappableLlm + LlmClientFactory.
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

    if enable_manage_tasks
        || enable_manage_agents
        || enable_manage_marketplace
        || enable_manage_triggers
        || enable_manage_terminal
        || enable_security_query
    {
        if let Some(storage) = storage {
            let core_registry = create_tool_registry(
                storage.skills.clone(),
                storage.memory.clone(),
                storage.chat_sessions.clone(),
                storage.shared_space.clone(),
                storage.secrets.clone(),
                storage.config.clone(),
                storage.agents.clone(),
                storage.agent_tasks.clone(),
                storage.triggers.clone(),
                storage.terminal_sessions.clone(),
                None,
            );
            let storage_backed_tools = [
                ("manage_tasks", enable_manage_tasks),
                ("manage_agents", enable_manage_agents),
                ("manage_marketplace", enable_manage_marketplace),
                ("manage_triggers", enable_manage_triggers),
                ("manage_terminal", enable_manage_terminal),
                ("security_query", enable_security_query),
            ];
            for (tool_name, enabled) in storage_backed_tools {
                if !enabled {
                    continue;
                }
                if let Some(tool) = core_registry.get(tool_name) {
                    builder.registry.register_arc(tool);
                } else {
                    warn!(
                        tool_name = tool_name,
                        "Tool was requested but not found in core registry"
                    );
                }
            }
        } else {
            for (tool_name, enabled) in [
                ("manage_tasks", enable_manage_tasks),
                ("manage_agents", enable_manage_agents),
                ("manage_marketplace", enable_manage_marketplace),
                ("manage_triggers", enable_manage_triggers),
                ("manage_terminal", enable_manage_terminal),
                ("security_query", enable_security_query),
            ] {
                if enabled {
                    warn!(
                        tool_name = tool_name,
                        "Storage is unavailable, skipping storage-backed tool"
                    );
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{
        effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    };
    use crate::storage::Storage;
    use tempfile::tempdir;

    #[test]
    fn test_manage_tasks_tool_registered_with_storage() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-tools.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec!["manage_tasks".to_string(), "manage_agents".to_string()];

        let registry = registry_from_allowlist(Some(&names), None, None, Some(&storage));
        assert!(registry.has("manage_tasks"));
        assert!(registry.has("manage_agents"));
    }

    #[test]
    fn test_manage_tasks_tool_skipped_without_storage() {
        let names = vec!["manage_tasks".to_string(), "manage_agents".to_string()];
        let registry = registry_from_allowlist(Some(&names), None, None, None);
        assert!(!registry.has("manage_tasks"));
        assert!(!registry.has("manage_agents"));
    }

    #[test]
    fn test_platform_tools_registered_with_storage() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("platform-tools.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec![
            "manage_marketplace".to_string(),
            "manage_triggers".to_string(),
            "manage_terminal".to_string(),
            "security_query".to_string(),
        ];

        let registry = registry_from_allowlist(Some(&names), None, None, Some(&storage));
        assert!(registry.has("manage_marketplace"));
        assert!(registry.has("manage_triggers"));
        assert!(registry.has("manage_terminal"));
        assert!(registry.has("security_query"));
    }

    #[test]
    fn test_main_agent_default_tools_include_transcribe_and_switch_model() {
        let tools = main_agent_default_tool_names();
        assert!(tools.iter().any(|name| name == "transcribe"));
        assert!(tools.iter().any(|name| name == "vision"));
        assert!(tools.iter().any(|name| name == "switch_model"));
        assert!(tools.iter().any(|name| name == "manage_agents"));
        assert!(tools.iter().any(|name| name == "manage_marketplace"));
        assert!(tools.iter().any(|name| name == "manage_triggers"));
        assert!(tools.iter().any(|name| name == "manage_terminal"));
        assert!(tools.iter().any(|name| name == "security_query"));
    }

    #[test]
    fn test_effective_main_agent_tool_names_merges_without_duplicates() {
        let extra = vec!["custom_tool".to_string(), "bash".to_string()];
        let merged = effective_main_agent_tool_names(Some(&extra));
        assert!(merged.iter().any(|name| name == "custom_tool"));
        assert_eq!(
            merged.iter().filter(|name| name.as_str() == "bash").count(),
            1
        );
    }
}
