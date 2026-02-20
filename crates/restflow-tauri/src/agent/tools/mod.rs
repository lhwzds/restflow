//! Unified tool registry for agent execution.
//!
//! Independent tools (bash, http, file, email, telegram) are imported from
//! restflow-core to avoid duplication. Subagent-dependent tools and
//! platform-specific tools (show_panel) remain local.

use std::sync::Arc;
use tracing::warn;

use crate::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};
use restflow_ai::LlmClient;
use restflow_core::services::tool_registry::create_tool_registry;
use restflow_core::storage::Storage;

pub use restflow_ai::tools::{
    PythonTool, RunPythonTool, SecretResolver, Tool, ToolOutput, ToolRegistry, TranscribeTool,
    VisionTool,
};

// Independent tools from restflow-core (canonical implementations).
pub use restflow_core::runtime::agent::tools::{
    BashConfig, BashTool, EmailTool, FileConfig, FileTool, HttpTool, TelegramTool,
};

// Subagent-dependent tools (use local SubagentDeps).
mod list_agents;
mod show_panel;
mod spawn;
mod spawn_agent;
mod use_skill;
mod wait_agents;

pub use list_agents::ListAgentsTool;
pub use show_panel::ShowPanelTool;
pub use spawn::{SpawnTool, SubagentSpawner};
pub use spawn_agent::SpawnAgentTool;
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
        "email",
        "telegram",
        "run_python",
        "transcribe",
        "vision",
        "spawn_agent",
        "wait_agents",
        "list_agents",
        "use_skill",
        "manage_background_agents",
        "manage_agents",
        "manage_marketplace",
        "manage_triggers",
        "manage_terminal",
        "security_query",
        "switch_model",
        "skill",
        "memory_search",
        "shared_space",
        "workspace_notes",
        "manage_secrets",
        "manage_config",
        "manage_sessions",
        "manage_memory",
        "manage_auth_profiles",
        "save_deliverable",
        "patch",
        "diagnostics",
        "web_search",
        "web_fetch",
        "jina_reader",
        "show_panel",
        "reply",
        "process",
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

    /// Add monty-backed python tools.
    pub fn with_python(mut self) -> Self {
        self.registry.register(RunPythonTool::new());
        self.registry.register(PythonTool::new());
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
/// - `email` -> `send_email`
/// - `telegram` -> `telegram_send`
/// - `http_request` -> `http`
/// - `read`/`write` -> `file` (write enables file writes)
/// - `python` <-> `run_python`
pub fn registry_from_allowlist(
    tool_names: Option<&[String]>,
    subagent_deps: Option<&SubagentDeps>,
    secret_resolver: Option<SecretResolver>,
    storage: Option<&Storage>,
    agent_id: Option<&str>,
    bash_config: Option<BashConfig>,
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
    let mut enable_manage_background_agents = false;
    let mut enable_manage_agents = false;
    let mut enable_manage_marketplace = false;
    let mut enable_manage_triggers = false;
    let mut enable_manage_terminal = false;
    let mut enable_security_query = false;
    let mut enable_skill = false;
    let mut enable_memory_search = false;
    let mut enable_shared_space = false;
    let mut enable_workspace_notes = false;
    let mut enable_manage_secrets = false;
    let mut enable_manage_config = false;
    let mut enable_manage_sessions = false;
    let mut enable_manage_memory = false;
    let mut enable_manage_auth_profiles = false;
    let mut enable_patch = false;
    let mut enable_diagnostics = false;
    let mut enable_file_memory = false;
    let mut enable_save_deliverable = false;

    for raw_name in tool_names {
        match raw_name.as_str() {
            "bash" => {
                builder = builder.with_bash(bash_config.clone().unwrap_or_default());
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
            "send_email" | "email" => {
                builder = builder.with_email();
            }
            "telegram_send" | "telegram" => {
                builder = builder.with_telegram();
            }
            "python" | "run_python" => {
                builder = builder.with_python();
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
            "manage_background_agents" => {
                enable_manage_background_agents = true;
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
            "skill" => {
                enable_skill = true;
            }
            "memory_search" => {
                enable_memory_search = true;
            }
            "shared_space" => {
                enable_shared_space = true;
            }
            "workspace_notes" => {
                enable_workspace_notes = true;
            }
            "manage_secrets" | "secrets" => {
                enable_manage_secrets = true;
            }
            "manage_config" | "config" => {
                enable_manage_config = true;
            }
            "manage_sessions" | "sessions" => {
                enable_manage_sessions = true;
            }
            "manage_memory" => {
                enable_manage_memory = true;
            }
            "manage_auth_profiles" | "auth_profiles" => {
                enable_manage_auth_profiles = true;
            }
            "patch" => {
                enable_patch = true;
            }
            "diagnostics" => {
                enable_diagnostics = true;
            }
            "save_to_memory" | "read_memory" | "list_memories" | "delete_memory" => {
                enable_file_memory = true;
            }
            "save_deliverable" => {
                enable_save_deliverable = true;
            }
            "web_search" => {
                let mut tool = restflow_ai::tools::WebSearchTool::new();
                if let Some(resolver) = secret_resolver.clone() {
                    tool = tool.with_secret_resolver(resolver);
                }
                builder.registry.register(tool);
            }
            "web_fetch" => {
                builder
                    .registry
                    .register(restflow_ai::tools::WebFetchTool::new());
            }
            "jina_reader" => {
                builder
                    .registry
                    .register(restflow_ai::tools::JinaReaderTool::new());
            }
            "switch_model" => {
                // Registered by callers that provide SwappableLlm + LlmClientFactory.
            }
            "show_panel" => {
                builder.registry.register(ShowPanelTool::new());
            }
            "reply" => {
                // Registered by callers that provide a ReplySender (e.g., ChatDispatcher).
            }
            "process" => {
                // Registered by callers that provide a ProcessRegistry.
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

    let any_storage_tool = enable_manage_background_agents
        || enable_manage_agents
        || enable_manage_marketplace
        || enable_manage_triggers
        || enable_manage_terminal
        || enable_security_query
        || enable_skill
        || enable_memory_search
        || enable_shared_space
        || enable_workspace_notes
        || enable_manage_secrets
        || enable_manage_config
        || enable_manage_sessions
        || enable_manage_memory
        || enable_manage_auth_profiles
        || enable_patch
        || enable_diagnostics
        || enable_file_memory
        || enable_save_deliverable;

    if any_storage_tool {
        if let Some(storage) = storage {
            let core_registry = create_tool_registry(
                storage.skills.clone(),
                storage.memory.clone(),
                storage.chat_sessions.clone(),
                storage.shared_space.clone(),
                storage.workspace_notes.clone(),
                storage.secrets.clone(),
                storage.config.clone(),
                storage.agents.clone(),
                storage.background_agents.clone(),
                storage.triggers.clone(),
                storage.terminal_sessions.clone(),
                storage.deliverables.clone(),
                None,
                agent_id.map(|s| s.to_string()),
            );
            let storage_backed_tools = [
                ("manage_background_agents", enable_manage_background_agents),
                ("manage_agents", enable_manage_agents),
                ("manage_marketplace", enable_manage_marketplace),
                ("manage_triggers", enable_manage_triggers),
                ("manage_terminal", enable_manage_terminal),
                ("security_query", enable_security_query),
                ("skill", enable_skill),
                ("memory_search", enable_memory_search),
                ("shared_space", enable_shared_space),
                ("workspace_notes", enable_workspace_notes),
                ("manage_secrets", enable_manage_secrets),
                ("manage_config", enable_manage_config),
                ("manage_sessions", enable_manage_sessions),
                ("manage_memory", enable_manage_memory),
                ("manage_auth_profiles", enable_manage_auth_profiles),
                ("patch", enable_patch),
                ("diagnostics", enable_diagnostics),
                ("save_to_memory", enable_file_memory),
                ("read_memory", enable_file_memory),
                ("list_memories", enable_file_memory),
                ("delete_memory", enable_file_memory),
                ("save_deliverable", enable_save_deliverable),
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
            let storage_backed_tools = [
                ("manage_background_agents", enable_manage_background_agents),
                ("manage_agents", enable_manage_agents),
                ("manage_marketplace", enable_manage_marketplace),
                ("manage_triggers", enable_manage_triggers),
                ("manage_terminal", enable_manage_terminal),
                ("security_query", enable_security_query),
                ("skill", enable_skill),
                ("memory_search", enable_memory_search),
                ("shared_space", enable_shared_space),
                ("workspace_notes", enable_workspace_notes),
                ("manage_secrets", enable_manage_secrets),
                ("manage_config", enable_manage_config),
                ("manage_sessions", enable_manage_sessions),
                ("manage_memory", enable_manage_memory),
                ("manage_auth_profiles", enable_manage_auth_profiles),
                ("patch", enable_patch),
                ("diagnostics", enable_diagnostics),
                ("save_to_memory", enable_file_memory),
                ("read_memory", enable_file_memory),
                ("list_memories", enable_file_memory),
                ("delete_memory", enable_file_memory),
                ("save_deliverable", enable_save_deliverable),
            ];
            for (tool_name, enabled) in storage_backed_tools {
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
        .with_email()
        .with_telegram()
        .with_python()
        .build()
}

#[cfg(test)]
mod tests {
    use super::{
        effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    };
    use restflow_core::storage::Storage;
    use tempfile::tempdir;

    #[test]
    fn test_manage_background_agents_tool_registered_with_storage() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-tools.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec![
            "manage_background_agents".to_string(),
            "manage_agents".to_string(),
        ];

        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None);
        assert!(registry.has("manage_background_agents"));
        assert!(registry.has("manage_agents"));
    }

    #[test]
    fn test_manage_background_agents_tool_skipped_without_storage() {
        let names = vec![
            "manage_background_agents".to_string(),
            "manage_agents".to_string(),
        ];
        let registry = registry_from_allowlist(Some(&names), None, None, None, None, None);
        assert!(!registry.has("manage_background_agents"));
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

        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None);
        assert!(registry.has("manage_marketplace"));
        assert!(registry.has("manage_triggers"));
        assert!(registry.has("manage_terminal"));
        assert!(registry.has("security_query"));
    }

    #[test]
    fn test_main_agent_default_tools_include_transcribe_and_switch_model() {
        let tools = main_agent_default_tool_names();
        assert!(tools.iter().any(|name| name == "run_python"));
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
    fn test_python_alias_and_run_python_are_both_registered() {
        let names = vec!["python".to_string()];
        let registry = registry_from_allowlist(Some(&names), None, None, None, None, None);
        assert!(registry.has("python"));
        assert!(registry.has("run_python"));
    }

    #[test]
    fn test_new_storage_tools_registered_with_storage() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("new-tools.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec![
            "skill".to_string(),
            "memory_search".to_string(),
            "shared_space".to_string(),
            "manage_secrets".to_string(),
            "manage_config".to_string(),
            "manage_sessions".to_string(),
            "manage_memory".to_string(),
            "manage_auth_profiles".to_string(),
            "patch".to_string(),
            "diagnostics".to_string(),
        ];

        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None);
        assert!(registry.has("skill"));
        assert!(registry.has("memory_search"));
        assert!(registry.has("shared_space"));
        assert!(registry.has("manage_secrets"));
        assert!(registry.has("manage_config"));
        assert!(registry.has("manage_sessions"));
        assert!(registry.has("manage_memory"));
        assert!(registry.has("manage_auth_profiles"));
        assert!(registry.has("patch"));
        assert!(registry.has("diagnostics"));
    }

    #[test]
    fn test_main_agent_default_tools_include_new_tools() {
        let tools = main_agent_default_tool_names();
        assert!(tools.iter().any(|name| name == "skill"));
        assert!(tools.iter().any(|name| name == "memory_search"));
        assert!(tools.iter().any(|name| name == "shared_space"));
        assert!(tools.iter().any(|name| name == "manage_secrets"));
        assert!(tools.iter().any(|name| name == "manage_config"));
        assert!(tools.iter().any(|name| name == "manage_sessions"));
        assert!(tools.iter().any(|name| name == "manage_memory"));
        assert!(tools.iter().any(|name| name == "manage_auth_profiles"));
        assert!(tools.iter().any(|name| name == "patch"));
        assert!(tools.iter().any(|name| name == "diagnostics"));
    }

    #[test]
    fn test_file_memory_tools_registered_with_agent_id() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("memory-tools.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec![
            "save_to_memory".to_string(),
            "read_memory".to_string(),
            "list_memories".to_string(),
            "delete_memory".to_string(),
        ];
        let registry = registry_from_allowlist(
            Some(&names),
            None,
            None,
            Some(&storage),
            Some("test-agent"),
            None,
        );
        assert!(registry.has("save_to_memory"));
        assert!(registry.has("read_memory"));
        assert!(registry.has("list_memories"));
        assert!(registry.has("delete_memory"));
    }

    #[test]
    fn test_file_memory_tools_registered_without_agent_id() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("memory-tools-no-agent.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec![
            "save_to_memory".to_string(),
            "read_memory".to_string(),
            "list_memories".to_string(),
            "delete_memory".to_string(),
        ];
        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None);
        assert!(registry.has("save_to_memory"));
        assert!(registry.has("read_memory"));
        assert!(registry.has("list_memories"));
        assert!(registry.has("delete_memory"));
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
