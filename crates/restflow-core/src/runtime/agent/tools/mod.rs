//! Unified tool registry for agent execution.
//!
//! Tool implementations live in `restflow-tools`. This module provides
//! assembly functions (`registry_from_allowlist`) that combine tools with
//! storage-backed services from `restflow-core`.

use std::sync::Arc;
use tracing::{debug, warn};

use crate::services::tool_registry::{SkillStorageProvider, create_tool_registry};
use crate::storage::Storage;
use restflow_ai::SkillProvider;

// Re-export tool types from restflow-tools
pub use restflow_tools::impls::{
    BashConfig, BashTool, DiscordTool, EmailTool, FileConfig, FileTool, HttpTool,
    ListAgentsTool, SlackTool, SpawnAgentTool, SpawnTool, TelegramTool,
    ToolRegistryBuilder, UseSkillTool, WaitAgentsTool,
    default_registry,
};
pub use restflow_tools::{PythonTool, RunPythonTool, TranscribeTool, VisionTool};

// Re-export core types from restflow-ai
pub use restflow_ai::tools::{SecretResolver, Tool, ToolOutput, ToolRegistry};
pub use restflow_ai::agent::{SubagentDeps, SubagentSpawner};

pub type ToolResult = ToolOutput;

pub fn secret_resolver_from_storage(storage: &Storage) -> SecretResolver {
    let secrets = storage.secrets.clone();
    Arc::new(move |key| secrets.get_secret(key).ok().flatten())
}

/// Default tools for main agents.
pub fn main_agent_default_tool_names() -> Vec<String> {
    vec![
        "bash",
        "file",
        "http",
        "email",
        "telegram",
        "discord",
        "slack",
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
        "manage_ops",
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

/// Build a tool registry filtered by an allowlist.
///
/// When `tool_names` is `None` or empty, returns an empty registry (secure default).
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
    let mut enable_manage_ops = false;
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
            "discord_send" | "discord" => {
                builder = builder.with_discord();
            }
            "slack_send" | "slack" => {
                builder = builder.with_slack();
            }
            "python" | "run_python" => {
                builder = builder.with_python();
            }
            "transcribe" => {
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_transcribe(resolver);
                } else {
                    warn!(tool_name = "transcribe", "Secret resolver missing, skipping");
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
                    debug!(tool_name = "spawn_agent", "Subagent dependencies missing, skipping");
                }
            }
            "wait_agents" => {
                if let Some(deps) = subagent_deps {
                    builder = builder.with_wait_agents(Arc::new(deps.clone()));
                } else {
                    debug!(tool_name = "wait_agents", "Subagent dependencies missing, skipping");
                }
            }
            "list_agents" => {
                if let Some(deps) = subagent_deps {
                    builder = builder.with_list_agents(Arc::new(deps.clone()));
                } else {
                    debug!(tool_name = "list_agents", "Subagent dependencies missing, skipping");
                }
            }
            "use_skill" => {
                if let Some(storage) = storage {
                    let provider: Arc<dyn SkillProvider> = Arc::new(SkillStorageProvider::new(storage.skills.clone()));
                    builder = builder.with_use_skill(provider);
                } else {
                    debug!(tool_name = "use_skill", "Storage missing, skipping");
                }
            }
            "manage_background_agents" => enable_manage_background_agents = true,
            "manage_agents" => enable_manage_agents = true,
            "manage_marketplace" => enable_manage_marketplace = true,
            "manage_triggers" => enable_manage_triggers = true,
            "manage_terminal" => enable_manage_terminal = true,
            "manage_ops" => enable_manage_ops = true,
            "security_query" => enable_security_query = true,
            "skill" => enable_skill = true,
            "memory_search" => enable_memory_search = true,
            "shared_space" => enable_shared_space = true,
            "workspace_notes" => enable_workspace_notes = true,
            "manage_secrets" | "secrets" => enable_manage_secrets = true,
            "manage_config" | "config" => enable_manage_config = true,
            "manage_sessions" | "sessions" => enable_manage_sessions = true,
            "manage_memory" => enable_manage_memory = true,
            "manage_auth_profiles" | "auth_profiles" => enable_manage_auth_profiles = true,
            "patch" => enable_patch = true,
            "diagnostics" => enable_diagnostics = true,
            "save_to_memory" | "read_memory" | "list_memories" | "delete_memory" => {
                enable_file_memory = true;
            }
            "save_deliverable" => enable_save_deliverable = true,
            "web_search" => {
                let mut tool = restflow_tools::WebSearchTool::new();
                if let Some(resolver) = secret_resolver.clone() {
                    tool = tool.with_secret_resolver(resolver);
                }
                builder.registry.register(tool);
            }
            "web_fetch" => {
                builder.registry.register(restflow_tools::WebFetchTool::new());
            }
            "jina_reader" => {
                builder.registry.register(restflow_tools::JinaReaderTool::new());
            }
            "switch_model" => {
                // Registered by callers that provide SwappableLlm + LlmClientFactory.
            }
            "reply" => {
                // Registered by callers that provide a ReplySender.
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

    // Register skills as callable tools
    if let Some(storage) = storage {
        let provider: Arc<dyn SkillProvider> = Arc::new(SkillStorageProvider::new(storage.skills.clone()));
        restflow_tools::register_skills(&mut builder.registry, provider);
    }

    let any_storage_tool = enable_manage_background_agents
        || enable_manage_agents
        || enable_manage_marketplace
        || enable_manage_triggers
        || enable_manage_terminal
        || enable_manage_ops
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
                ("manage_ops", enable_manage_ops),
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
                    warn!(tool_name = tool_name, "Tool was requested but not found in core registry");
                }
            }
        } else {
            let storage_backed_tools = [
                ("manage_background_agents", enable_manage_background_agents),
                ("manage_agents", enable_manage_agents),
                ("manage_marketplace", enable_manage_marketplace),
                ("manage_triggers", enable_manage_triggers),
                ("manage_terminal", enable_manage_terminal),
                ("manage_ops", enable_manage_ops),
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
                    warn!(tool_name = tool_name, "Storage is unavailable, skipping storage-backed tool");
                }
            }
        }
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::{
        effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    };
    use crate::storage::Storage;
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
            "manage_ops".to_string(),
            "security_query".to_string(),
        ];

        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None);
        assert!(registry.has("manage_marketplace"));
        assert!(registry.has("manage_triggers"));
        assert!(registry.has("manage_terminal"));
        assert!(registry.has("manage_ops"));
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
        assert!(tools.iter().any(|name| name == "manage_ops"));
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
