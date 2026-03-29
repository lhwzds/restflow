//! Unified tool registry for agent execution.
//!
//! Tool implementations live in `restflow-tools`. This module provides
//! assembly functions (`registry_from_allowlist`) that combine tools with
//! storage-backed services from `restflow-core`.

pub(crate) mod assembly;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

use self::assembly::{
    populate_known_tools_from_registry, register_bash_execution_tool, register_file_execution_tool,
    register_http_execution_tool, register_python_execution_tools,
    register_send_email_execution_tool,
};
use crate::lsp::LspManager;
use crate::memory::UnifiedSearchEngine;
use crate::services::adapters::*;
use crate::storage::Storage;
use restflow_storage::{AgentSettings, ApiSettings};
use restflow_traits::security::SecurityGate;
use restflow_traits::skill::SkillProvider;
use restflow_traits::store::DiagnosticsProvider;

// Re-export tool types from restflow-tools
pub use restflow_tools::impls::{
    BashConfig, BashTool, DiscordTool, EmailTool, FileConfig, FileTool, HttpTool,
    ListSubagentsTool, SlackTool, SpawnSubagentTool, SpawnTool, TelegramTool, ToolRegistryBuilder,
    UseSkillTool, WaitSubagentsTool, default_registry,
};
pub use restflow_tools::{PythonTool, RunPythonTool, TranscribeConfig, TranscribeTool, VisionTool};

// Re-export core types from restflow-ai
pub use restflow_ai::agent::{SubagentDeps, SubagentManagerImpl, SubagentSpawner};
pub use restflow_ai::tools::{SecretResolver, Tool, ToolOutput, ToolRegistry};
pub use restflow_traits::SubagentManager;

pub type ToolResult = ToolOutput;
const DEFAULT_SECURITY_AGENT_ID: &str = "unknown-agent";
const DEFAULT_SECURITY_TASK_ID: &str = "tool-registry";

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
        "browser",
        "transcribe",
        "vision",
        "spawn_subagent",
        "wait_subagents",
        "list_subagents",
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
    agent_id: Option<&str>,
    bash_config: Option<BashConfig>,
    workspace_root: Option<&Path>,
) -> anyhow::Result<ToolRegistry> {
    registry_from_allowlist_with_security_gate(
        tool_names,
        subagent_manager,
        secret_resolver,
        storage,
        agent_id,
        bash_config,
        workspace_root,
        None,
    )
}

/// Build a tool registry filtered by an allowlist with an optional security gate.
///
/// When `tool_names` is `None` or empty, returns an empty registry (secure default).
/// Storage-backed tools are created directly via [`ToolRegistryBuilder`] methods,
/// avoiding the need to build a full core registry and cherry-pick from it.
#[allow(clippy::too_many_arguments)]
pub fn registry_from_allowlist_with_security_gate(
    tool_names: Option<&[String]>,
    subagent_manager: Option<Arc<dyn SubagentManager>>,
    secret_resolver: Option<SecretResolver>,
    storage: Option<&Storage>,
    agent_id: Option<&str>,
    bash_config: Option<BashConfig>,
    workspace_root: Option<&Path>,
    security_gate: Option<Arc<dyn SecurityGate>>,
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
    let mut allowlisted_skill_ids: Vec<String> = Vec::new();
    let mut recorded_skill_ids: HashSet<String> = HashSet::new();
    let effective_config = storage.and_then(|value| {
        value
            .config
            .get_effective_config_for_workspace(workspace_root)
            .ok()
    });

    // Pre-create shared diagnostics provider when any of diagnostics/edit/multiedit
    // are in the allowlist, so they all share the same LspManager instance.
    let needs_diag = tool_names
        .iter()
        .any(|n| matches!(n.as_str(), "diagnostics" | "edit" | "multiedit"));
    let shared_diagnostics: Option<Arc<dyn DiagnosticsProvider>> =
        if needs_diag && let Some(root) = workspace_root {
            Some(Arc::new(LspManager::new(root.to_path_buf())))
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
                let config = bash_config.clone().unwrap_or_default();
                builder = register_bash_execution_tool(
                    builder,
                    config,
                    security_gate.clone(),
                    agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                    DEFAULT_SECURITY_TASK_ID,
                );
            }
            "file" | "read" => {
                allow_file = true;
            }
            "write" => {
                allow_file = true;
                allow_file_write = true;
            }
            "http" | "http_request" => {
                builder = register_http_execution_tool(
                    builder,
                    security_gate.clone(),
                    agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                    DEFAULT_SECURITY_TASK_ID,
                )?;
            }
            "send_email" | "email" => {
                builder = register_send_email_execution_tool(
                    builder,
                    security_gate.clone(),
                    agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                    DEFAULT_SECURITY_TASK_ID,
                );
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
                builder = register_python_execution_tools(
                    builder,
                    security_gate.clone(),
                    agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                    DEFAULT_SECURITY_TASK_ID,
                );
            }
            "browser" => {
                let timeout_secs = effective_config
                    .as_ref()
                    .map(|config| config.agent.browser_timeout_secs)
                    .unwrap_or_else(|| AgentSettings::default().browser_timeout_secs);
                builder = builder.with_browser_timeout(timeout_secs)?;
            }
            "transcribe" => {
                if let Some(resolver) = secret_resolver.clone() {
                    let config = workspace_root
                        .map(TranscribeConfig::for_workspace_root)
                        .unwrap_or_default();
                    builder = builder.with_transcribe_config(resolver, config)?;
                } else {
                    warn!(
                        tool_name = "transcribe",
                        "Secret resolver missing, skipping"
                    );
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
                let default_num_results = effective_config
                    .as_ref()
                    .map(|config| config.api_defaults.web_search_num_results)
                    .unwrap_or_else(|| ApiSettings::default().web_search_num_results);
                if let Some(resolver) = secret_resolver.clone() {
                    builder = builder.with_web_search_with_resolver_and_defaults(
                        resolver,
                        default_num_results,
                    )?;
                } else {
                    builder = builder.with_web_search_with_defaults(default_num_results)?;
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
                    let timeout_ms = effective_config
                        .as_ref()
                        .map(|config| config.api_defaults.diagnostics_timeout_ms)
                        .unwrap_or_else(|| ApiSettings::default().diagnostics_timeout_ms);
                    builder = builder.with_diagnostics_with_timeout(diag.clone(), timeout_ms);
                }
            }
            "security_query" => {
                let provider = if let Some(storage) = storage {
                    Arc::new(SecurityQueryProviderAdapter::with_config_storage(Arc::new(
                        storage.config.clone(),
                    )))
                } else {
                    Arc::new(SecurityQueryProviderAdapter::new())
                };
                builder = builder.with_security_query(provider);
            }
            "patch" => {
                builder = builder.with_patch_and_base_dir(workspace_root.map(Path::to_path_buf));
            }
            "edit" => {
                builder = builder.with_edit_and_diagnostics_and_base_dir(
                    shared_diagnostics.clone(),
                    workspace_root.map(Path::to_path_buf),
                );
            }
            "multiedit" => {
                builder = builder.with_multiedit_and_diagnostics_and_base_dir(
                    shared_diagnostics.clone(),
                    workspace_root.map(Path::to_path_buf),
                );
            }

            // --- Subagent tools ---
            "spawn_subagent" => {
                if let Some(manager) = &subagent_manager {
                    if let Some(store) = storage {
                        builder = builder.with_spawn_subagent_with_store(
                            manager.clone(),
                            Arc::new(KvStoreAdapter::new(store.kv_store.clone(), None)),
                        );
                    } else {
                        builder = builder.with_spawn_subagent(manager.clone());
                    }
                } else {
                    debug!(
                        tool_name = "spawn_subagent",
                        "Subagent manager missing, skipping"
                    );
                }
            }
            "wait_subagents" => {
                if let Some(manager) = &subagent_manager {
                    builder = builder.with_wait_subagents(manager.clone());
                } else {
                    debug!(
                        tool_name = "wait_subagents",
                        "Subagent manager missing, skipping"
                    );
                }
            }
            "list_subagents" => {
                if let Some(manager) = &subagent_manager {
                    builder = builder.with_list_subagents(manager.clone());
                } else {
                    debug!(
                        tool_name = "list_subagents",
                        "Subagent manager missing, skipping"
                    );
                }
            }
            "use_skill" => {
                if let Some(storage) = storage {
                    let provider: Arc<dyn SkillProvider> =
                        Arc::new(SkillStorageProvider::new(storage.skills.clone()));
                    builder = if let Some(gate) = security_gate.clone() {
                        builder.with_use_skill_with_security(
                            provider,
                            gate,
                            agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                            DEFAULT_SECURITY_TASK_ID,
                        )
                    } else {
                        builder.with_use_skill(provider)
                    };
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
                    builder.with_background_agent_and_kv(
                        store,
                        Arc::new(KvStoreAdapter::new(s.kv_store.clone(), None)),
                    )
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
                    let registry_defaults = effective_config
                        .as_ref()
                        .map(|config| config.registry_defaults.clone())
                        .unwrap_or_default();
                    builder.with_marketplace(Arc::new(MarketplaceStoreAdapter::new_with_defaults(
                        s.skills.clone(),
                        registry_defaults,
                    )))
                });
            }
            "manage_triggers" => {
                with_storage!(storage, "manage_triggers", builder, |s| {
                    builder.with_trigger(Arc::new(TriggerStoreAdapter::new(s.triggers.clone())))
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
                    let provider = Arc::new(SkillStorageProvider::new(s.skills.clone()));
                    if let Some(gate) = security_gate.clone() {
                        builder.with_skill_tool_with_security(
                            provider,
                            gate,
                            agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                            DEFAULT_SECURITY_TASK_ID,
                        )
                    } else {
                        builder.with_skill_tool(provider)
                    }
                });
            }
            "memory_search" => {
                with_storage!(storage, "memory_search", builder, |s| {
                    let engine =
                        UnifiedSearchEngine::new(s.memory.clone(), s.chat_sessions.clone());
                    builder.with_unified_search(Arc::new(UnifiedMemorySearchAdapter::new(engine)))
                });
            }
            "kv_store" => {
                with_storage!(storage, "kv_store", builder, |s| {
                    builder.with_kv_store(Arc::new(KvStoreAdapter::new(s.kv_store.clone(), None)))
                });
            }
            "work_items" => {
                with_storage!(storage, "work_items", builder, |s| {
                    builder.with_work_items(Arc::new(DbWorkItemAdapter::new(s.work_items.clone())))
                });
            }
            "manage_secrets" | "secrets" => {
                with_storage!(storage, "manage_secrets", builder, |s| {
                    builder.with_secrets(Arc::new(SecretStoreAdapter::new(Arc::new(
                        s.secrets.clone(),
                    ))))
                });
            }
            "manage_config" | "config" => {
                with_storage!(storage, "manage_config", builder, |s| {
                    builder.with_config(Arc::new(ConfigStoreAdapter::new(Arc::new(
                        s.config.clone(),
                    ))))
                });
            }
            "manage_sessions" | "sessions" => {
                with_storage!(storage, "manage_sessions", builder, |s| {
                    builder.with_session(Arc::new(SessionStorageAdapter::new(
                        s.sessions.clone(),
                        s.agents.clone(),
                        s.background_agents.clone(),
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
                builder = builder.with_glob_and_base_dir(workspace_root.map(Path::to_path_buf));
            }
            "grep" => {
                builder = builder.with_grep_and_base_dir(workspace_root.map(Path::to_path_buf));
            }
            "task_list" => {
                with_storage!(storage, "task_list", builder, |s| {
                    builder.with_task_list(Arc::new(DbWorkItemAdapter::new(s.work_items.clone())))
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
                if let Some(store) = storage {
                    match store.skills.exists(unknown) {
                        Ok(true) => {
                            if recorded_skill_ids.insert(unknown.to_string()) {
                                allowlisted_skill_ids.push(unknown.to_string());
                            }
                            continue;
                        }
                        Ok(false) => {}
                        Err(err) => {
                            warn!(
                                tool_name = %unknown,
                                error = %err,
                                "Failed to verify skill while building registry"
                            );
                        }
                    }
                }
                warn!(tool_name = %unknown, "Configured tool not found in registry, skipping");
            }
        }
    }

    if allow_file {
        let mut file_config = workspace_root
            .map(FileConfig::for_workspace_root)
            .unwrap_or_default();
        file_config.allow_write = allow_file_write;
        builder = register_file_execution_tool(
            builder,
            file_config,
            security_gate.clone(),
            agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
            DEFAULT_SECURITY_TASK_ID,
        );
    }

    // Register allowlisted skills as callable tools
    if let Some(storage) = storage {
        if !allowlisted_skill_ids.is_empty() {
            let provider: Arc<dyn SkillProvider> =
                Arc::new(SkillStorageProvider::new(storage.skills.clone()));
            register_allowlisted_skill_tools(
                &mut builder.registry,
                provider,
                &allowlisted_skill_ids,
                security_gate,
                agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                DEFAULT_SECURITY_TASK_ID,
            );
        }
    } else if !allowlisted_skill_ids.is_empty() {
        warn!(
            count = allowlisted_skill_ids.len(),
            "Skill entries found in allowlist but storage unavailable; skipping"
        );
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
    populate_known_tools_from_registry(&known_tools, &registry, None);

    Ok(registry)
}

fn register_allowlisted_skill_tools(
    registry: &mut ToolRegistry,
    provider: Arc<dyn SkillProvider>,
    skill_ids: &[String],
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) {
    if skill_ids.is_empty() {
        return;
    }

    let info_by_id: HashMap<String, restflow_traits::skill::SkillInfo> = provider
        .list_skills()
        .into_iter()
        .map(|info| (info.id.clone(), info))
        .collect();

    for skill_id in skill_ids {
        let Some(info) = info_by_id.get(skill_id) else {
            warn!(skill_id = %skill_id, "Allowlisted skill not found, skipping");
            continue;
        };
        let tool = if let Some(gate) = security_gate.as_ref() {
            restflow_tools::SkillAsTool::new(info.clone(), provider.clone()).with_security(
                gate.clone(),
                agent_id,
                task_id,
            )
        } else {
            restflow_tools::SkillAsTool::new(info.clone(), provider.clone())
        };
        registry.register(tool);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    };
    use crate::models::Skill;
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
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None, None)
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
            registry_from_allowlist(Some(&names), None, None, None, None, None, None).unwrap();
        assert!(!registry.has("manage_background_agents"));
        assert!(!registry.has("manage_agents"));
    }

    #[test]
    fn test_skills_only_registered_when_allowlisted() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-skills.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");

        let alpha = Skill::new(
            "alpha-skill".to_string(),
            "Alpha".to_string(),
            None,
            None,
            "# alpha".to_string(),
        );
        let beta = Skill::new(
            "beta-skill".to_string(),
            "Beta".to_string(),
            None,
            None,
            "# beta".to_string(),
        );
        storage.skills.create(&alpha).expect("create alpha");
        storage.skills.create(&beta).expect("create beta");

        let base_allowlist = vec!["use_skill".to_string()];
        let registry = registry_from_allowlist(
            Some(&base_allowlist),
            None,
            None,
            Some(&storage),
            None,
            None,
            None,
        )
        .expect("registry should build");
        assert!(
            !registry.has("alpha-skill"),
            "skills must not be auto-registered"
        );
        assert!(!registry.has("beta-skill"));

        let scoped_allowlist = vec![
            "use_skill".to_string(),
            "skill".to_string(),
            "alpha-skill".to_string(),
        ];
        let scoped_registry = registry_from_allowlist(
            Some(&scoped_allowlist),
            None,
            None,
            Some(&storage),
            None,
            None,
            None,
        )
        .expect("registry should build with allowlisted skill");
        assert!(scoped_registry.has("alpha-skill"));
        assert!(
            !scoped_registry.has("beta-skill"),
            "non-allowlisted skills stay unavailable"
        );
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
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None, None)
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
        assert!(tools.iter().any(|name| name == "browser"));
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
            registry_from_allowlist(Some(&names), None, None, None, None, None, None).unwrap();
        assert!(registry.has("python"));
        assert!(registry.has("run_python"));
    }

    #[tokio::test]
    async fn test_filesystem_tools_require_workspace_root_when_unset() {
        let names = vec!["file".to_string(), "glob".to_string(), "grep".to_string()];
        let registry =
            registry_from_allowlist(Some(&names), None, None, None, None, None, None).unwrap();

        let file_result = registry
            .get("file")
            .unwrap()
            .execute(serde_json::json!({
                "action": "read",
                "path": "relative.txt"
            }))
            .await
            .unwrap();
        assert!(!file_result.success);
        assert!(
            file_result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("workspace root or base directory")
        );

        let glob_result = registry
            .get("glob")
            .unwrap()
            .execute(serde_json::json!({
                "pattern": "**/*.rs"
            }))
            .await
            .unwrap();
        assert!(!glob_result.success);

        let grep_result = registry
            .get("grep")
            .unwrap()
            .execute(serde_json::json!({
                "pattern": "hello"
            }))
            .await
            .unwrap();
        assert!(!grep_result.success);
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

    #[test]
    fn test_registry_from_allowlist_uses_configured_api_defaults() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-api-defaults.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let mut config = storage
            .config
            .get_config()
            .expect("config should load")
            .expect("config should exist");
        config.api_defaults.web_search_num_results = 7;
        config.api_defaults.diagnostics_timeout_ms = 9_000;
        storage
            .config
            .update_config(config)
            .expect("config should update");

        let names = vec!["web_search".to_string(), "diagnostics".to_string()];
        let registry = registry_from_allowlist(
            Some(&names),
            None,
            None,
            Some(&storage),
            None,
            None,
            Some(dir.path()),
        )
        .unwrap();

        let web_search_schema = registry
            .get("web_search")
            .expect("web_search tool should exist")
            .parameters_schema();
        assert_eq!(web_search_schema["properties"]["num_results"]["default"], 7);

        let diagnostics_schema = registry
            .get("diagnostics")
            .expect("diagnostics tool should exist")
            .parameters_schema();
        assert_eq!(
            diagnostics_schema["properties"]["timeout_ms"]["default"],
            9_000
        );
    }

    #[test]
    fn test_diagnostics_tool_requires_workspace_root() {
        let names = vec!["diagnostics".to_string()];
        let registry =
            registry_from_allowlist(Some(&names), None, None, None, None, None, None).unwrap();
        assert!(
            !registry.has("diagnostics"),
            "diagnostics should not be registered without an explicit workspace root"
        );
    }
}
