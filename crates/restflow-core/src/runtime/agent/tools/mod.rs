//! Unified tool registry for agent execution.
//!
//! Tool implementations live in `restflow-tools`. This module provides
//! assembly functions (`registry_from_allowlist`) that combine tools with
//! storage-backed services from `restflow-core`.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

use crate::lsp::LspManager;
use crate::memory::UnifiedSearchEngine;
use crate::services::adapters::*;
use crate::storage::Storage;
use restflow_traits::skill::SkillProvider;
use restflow_traits::store::DiagnosticsProvider;

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
pub use restflow_ai::agent::{SubagentDeps, SubagentManagerImpl, SubagentSpawner};
pub use restflow_traits::SubagentManager;

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
        "kv_store",
        "manage_secrets",
        "manage_config",
        "manage_sessions",
        "manage_memory",
        "manage_auth_profiles",
        "save_deliverable",
        "edit",
        "multiedit",
        "patch",
        "diagnostics",
        "web_search",
        "web_fetch",
        "jina_reader",
        "reply",
        "process",
        "glob",
        "grep",
        "task_list",
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
/// Storage-backed tools are created directly via [`ToolRegistryBuilder`] methods,
/// avoiding the need to build a full core registry and cherry-pick from it.
pub fn registry_from_allowlist(
    tool_names: Option<&[String]>,
    subagent_manager: Option<Arc<dyn SubagentManager>>,
    secret_resolver: Option<SecretResolver>,
    storage: Option<&Storage>,
    _agent_id: Option<&str>,
    bash_config: Option<BashConfig>,
) -> anyhow::Result<ToolRegistry> {
    let Some(tool_names) = tool_names else {
        return Ok(ToolRegistry::new());
    };

    if tool_names.is_empty() {
        return Ok(ToolRegistry::new());
    }

    let mut builder = ToolRegistryBuilder::new();
    let mut allow_file = false;
    let mut allow_file_write = false;
    let known_tools = Arc::new(RwLock::new(HashSet::new()));

    // Pre-create shared diagnostics provider when any of diagnostics/edit/multiedit
    // are in the allowlist, so they all share the same LspManager instance.
    let needs_diag = tool_names
        .iter()
        .any(|n| matches!(n.as_str(), "diagnostics" | "edit" | "multiedit"));
    let shared_diagnostics: Option<Arc<dyn DiagnosticsProvider>> = if needs_diag {
        let root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        Some(Arc::new(LspManager::new(root)))
    } else {
        None
    };

    /// Register a storage-backed tool, warning if storage is unavailable.
    macro_rules! with_storage {
        ($storage:expr, $tool_name:expr, $builder:ident, |$s:ident| $body:expr) => {
            if let Some($s) = $storage {
                $builder = $body;
            } else {
                warn!(tool_name = $tool_name, "Storage unavailable, skipping");
            }
        };
    }

    for raw_name in tool_names {
        match raw_name.as_str() {
            // --- Simple tools (no storage required) ---
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
                builder = builder.with_http()?;
            }
            "send_email" | "email" => {
                builder = builder.with_email();
            }
            "telegram_send" | "telegram" => {
                builder = builder.with_telegram()?;
            }
            "discord_send" | "discord" => {
                builder = builder.with_discord()?;
            }
            "slack_send" | "slack" => {
                builder = builder.with_slack()?;
            }
            "python" | "run_python" => {
                builder = builder.with_python();
            }
            "transcribe" => {
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_transcribe(resolver)?;
                } else {
                    warn!(tool_name = "transcribe", "Secret resolver missing, skipping");
                }
            }
            "vision" => {
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_vision(resolver)?;
                } else {
                    warn!(tool_name = "vision", "Secret resolver missing, skipping");
                }
            }
            "web_search" => {
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_web_search_with_resolver(resolver)?;
                } else {
                    builder = builder.with_web_search()?;
                }
            }
            "web_fetch" => {
                builder = builder.with_web_fetch();
            }
            "jina_reader" => {
                builder = builder.with_jina_reader()?;
            }
            "diagnostics" => {
                if let Some(diag) = &shared_diagnostics {
                    builder = builder.with_diagnostics(diag.clone());
                }
            }
            "security_query" => {
                builder =
                    builder.with_security_query(Arc::new(SecurityQueryProviderAdapter));
            }
            "patch" => {
                builder = builder.with_patch();
            }
            "edit" => {
                builder = builder.with_edit_and_diagnostics(shared_diagnostics.clone());
            }
            "multiedit" => {
                builder = builder.with_multiedit_and_diagnostics(shared_diagnostics.clone());
            }

            // --- Subagent tools ---
            "spawn_agent" => {
                if let Some(manager) = &subagent_manager {
                    builder = builder.with_spawn_agent(manager.clone());
                } else {
                    debug!(tool_name = "spawn_agent", "Subagent manager missing, skipping");
                }
            }
            "wait_agents" => {
                if let Some(manager) = &subagent_manager {
                    builder = builder.with_wait_agents(manager.clone());
                } else {
                    debug!(tool_name = "wait_agents", "Subagent manager missing, skipping");
                }
            }
            "list_agents" => {
                if let Some(manager) = &subagent_manager {
                    builder = builder.with_list_agents(manager.clone());
                } else {
                    debug!(tool_name = "list_agents", "Subagent manager missing, skipping");
                }
            }
            "use_skill" => {
                if let Some(storage) = storage {
                    let provider: Arc<dyn SkillProvider> =
                        Arc::new(SkillStorageProvider::new(storage.skills.clone()));
                    builder = builder.with_use_skill(provider);
                } else {
                    debug!(tool_name = "use_skill", "Storage missing, skipping");
                }
            }

            // --- Storage-backed tools ---
            "manage_background_agents" => {
                with_storage!(storage, "manage_background_agents", builder, |s| {
                    let store = Arc::new(BackgroundAgentStoreAdapter::new(
                        s.background_agents.clone(),
                        s.agents.clone(),
                        s.deliverables.clone(),
                    ));
                    builder.with_background_agent(store)
                });
            }
            "manage_agents" => {
                with_storage!(storage, "manage_agents", builder, |s| {
                    let store = Arc::new(AgentStoreAdapter::new(
                        s.agents.clone(),
                        s.skills.clone(),
                        s.secrets.clone(),
                        s.background_agents.clone(),
                        known_tools.clone(),
                    ));
                    builder.with_agent_crud(store)
                });
            }
            "manage_marketplace" => {
                with_storage!(storage, "manage_marketplace", builder, |s| {
                    builder.with_marketplace(Arc::new(MarketplaceStoreAdapter::new(
                        s.skills.clone(),
                    )))
                });
            }
            "manage_triggers" => {
                with_storage!(storage, "manage_triggers", builder, |s| {
                    builder.with_trigger(Arc::new(TriggerStoreAdapter::new(
                        s.triggers.clone(),
                    )))
                });
            }
            "manage_terminal" => {
                with_storage!(storage, "manage_terminal", builder, |s| {
                    builder.with_terminal(Arc::new(TerminalStoreAdapter::new(
                        s.terminal_sessions.clone(),
                    )))
                });
            }
            "manage_ops" => {
                with_storage!(storage, "manage_ops", builder, |s| {
                    builder.with_ops(Arc::new(OpsProviderAdapter::new(
                        s.background_agents.clone(),
                        s.chat_sessions.clone(),
                    )))
                });
            }
            "skill" => {
                with_storage!(storage, "skill", builder, |s| {
                    builder.with_skill_tool(Arc::new(SkillStorageProvider::new(
                        s.skills.clone(),
                    )))
                });
            }
            "memory_search" => {
                with_storage!(storage, "memory_search", builder, |s| {
                    let engine = UnifiedSearchEngine::new(
                        s.memory.clone(),
                        s.chat_sessions.clone(),
                    );
                    builder.with_unified_search(Arc::new(UnifiedMemorySearchAdapter::new(
                        engine,
                    )))
                });
            }
            "kv_store" => {
                with_storage!(storage, "kv_store", builder, |s| {
                    builder.with_kv_store(Arc::new(KvStoreAdapter::new(
                        s.kv_store.clone(),
                        None,
                    )))
                });
            }
            "work_items" => {
                with_storage!(storage, "work_items", builder, |s| {
                    builder.with_work_items(Arc::new(DbWorkItemAdapter::new(
                        s.work_items.clone(),
                    )))
                });
            }
            "manage_secrets" | "secrets" => {
                with_storage!(storage, "manage_secrets", builder, |s| {
                    builder.with_secrets(Arc::new(s.secrets.clone()))
                });
            }
            "manage_config" | "config" => {
                with_storage!(storage, "manage_config", builder, |s| {
                    builder.with_config(Arc::new(s.config.clone()))
                });
            }
            "manage_sessions" | "sessions" => {
                with_storage!(storage, "manage_sessions", builder, |s| {
                    builder.with_session(Arc::new(SessionStorageAdapter::new(
                        s.chat_sessions.clone(),
                        s.agents.clone(),
                    )))
                });
            }
            "manage_memory" => {
                with_storage!(storage, "manage_memory", builder, |s| {
                    builder.with_memory_management(Arc::new(MemoryManagerAdapter::new(
                        s.memory.clone(),
                    )))
                });
            }
            "manage_auth_profiles" | "auth_profiles" => {
                with_storage!(storage, "manage_auth_profiles", builder, |s| {
                    builder.with_auth_profile(Arc::new(AuthProfileStorageAdapter::new(
                        s.secrets.clone(),
                    )))
                });
            }
            "save_to_memory" | "read_memory" | "list_memories" | "delete_memory" => {
                // Register all 4 memory CRUD tools at once (idempotent via has() check)
                if !builder.registry.has("save_to_memory") {
                    with_storage!(storage, raw_name.as_str(), builder, |s| {
                        builder.with_memory_store(Arc::new(DbMemoryStoreAdapter::new(
                            s.memory.clone(),
                        )))
                    });
                }
            }
            "save_deliverable" => {
                with_storage!(storage, "save_deliverable", builder, |s| {
                    builder.with_deliverable(Arc::new(DeliverableStoreAdapter::new(
                        s.deliverables.clone(),
                    )))
                });
            }

            // --- Search tools ---
            "glob" => {
                builder = builder.with_glob();
            }
            "grep" => {
                builder = builder.with_grep();
            }
            "task_list" => {
                with_storage!(storage, "task_list", builder, |s| {
                    builder.with_task_list(Arc::new(DbWorkItemAdapter::new(
                        s.work_items.clone(),
                    )))
                });
            }

            // --- Batch tool (registered post-build, see below) ---
            "batch" => {
                // Handled after builder.build() since BatchTool needs Arc<ToolRegistry>.
            }

            // --- Caller-registered tools (placeholders) ---
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
        let provider: Arc<dyn SkillProvider> =
            Arc::new(SkillStorageProvider::new(storage.skills.clone()));
        restflow_tools::register_skills(&mut builder.registry, provider);
    }

    // Check if batch tool was requested
    let wants_batch = tool_names.iter().any(|n| n == "batch");

    let mut registry = builder.build();

    // Batch tool needs Arc<ToolRegistry> — register it post-build as a two-phase step.
    if wants_batch {
        let registry_arc = Arc::new(std::mem::take(&mut registry));
        registry = ToolRegistry::new();
        // Move all tools from the Arc'd registry back, plus batch
        for name in registry_arc.list() {
            if let Some(tool) = registry_arc.get(name) {
                registry.register_arc(tool);
            }
        }
        registry.register(restflow_tools::BatchTool::new(registry_arc));
    }

    // Populate known_tools for AgentStoreAdapter validation
    if let Ok(mut known) = known_tools.write() {
        *known = registry
            .list()
            .into_iter()
            .map(|name| name.to_string())
            .collect::<HashSet<_>>();
    }

    Ok(registry)
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
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None)
                .unwrap();
        assert!(registry.has("manage_background_agents"));
        assert!(registry.has("manage_agents"));
    }

    #[test]
    fn test_manage_background_agents_tool_skipped_without_storage() {
        let names = vec![
            "manage_background_agents".to_string(),
            "manage_agents".to_string(),
        ];
        let registry =
            registry_from_allowlist(Some(&names), None, None, None, None, None).unwrap();
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
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None)
                .unwrap();
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
        let registry =
            registry_from_allowlist(Some(&names), None, None, None, None, None).unwrap();
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
