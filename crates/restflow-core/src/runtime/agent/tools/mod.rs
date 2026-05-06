//! Unified tool registry for agent execution.
//!
//! Tool implementations live in `restflow-tools`. This module provides
//! assembly functions (`registry_from_allowlist`) that combine tools with
//! storage-backed services from `restflow-core`.

pub(crate) mod assembly;
pub mod skill_activation;

use std::path::Path;
use std::sync::Arc;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::{Mutex, OnceLock};
use tracing::{debug, warn};

use self::assembly::{
    build_agent_crud_components, build_runtime_assessor, build_task_store_runtime_components,
    populate_known_tools_from_registry, register_bash_execution_tool, register_file_execution_tool,
    register_management_tools, register_subagent_management_tools,
};
use crate::lsp::LspManager;
use crate::memory::UnifiedSearchEngine;
use crate::services::adapters::*;
use crate::storage::Storage;
use restflow_storage::ApiSettings;
use restflow_traits::SubagentManager;
use restflow_traits::security::SecurityGate;
use restflow_traits::skill::SkillProvider;
use restflow_traits::store::{
    DiagnosticsProvider, MANAGE_TASKS_TOOL_NAME, is_task_management_tool_name,
};

// Re-export tool types from restflow-tools
pub use restflow_tools::impls::{
    BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool, RunSkillTool,
    SpawnSubagentTool, SpawnTool, ToolRegistryBuilder, WaitSubagentsTool, default_registry,
};

pub use restflow_ai::tools::{SecretResolver, Tool, ToolOutput, ToolRegistry};
pub use skill_activation::{
    SkillActivationIssue, SkillActivationIssueCategory, SkillActivationPolicy,
    SkillActivationResult, effective_tool_allowlist_for_turn,
    resolve_skill_activated_tool_allowlist,
};

pub type ToolResult = ToolOutput;
const DEFAULT_SECURITY_AGENT_ID: &str = "unknown-agent";
const DEFAULT_SECURITY_TASK_ID: &str = "tool-registry";

fn composite_skill_provider(storage: Option<&Storage>) -> Arc<dyn SkillProvider> {
    let _ = storage;
    Arc::new(SkrunSkillProvider::default())
}

#[cfg(any(test, feature = "test-utils"))]
type TestToolOverrideMap = std::collections::HashMap<String, Arc<dyn Tool>>;

#[cfg(any(test, feature = "test-utils"))]
fn test_tool_override_slot() -> &'static Mutex<Option<TestToolOverrideMap>> {
    static SLOT: OnceLock<Mutex<Option<TestToolOverrideMap>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(any(test, feature = "test-utils"))]
pub struct TestToolOverrideGuard {
    previous: Option<TestToolOverrideMap>,
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for TestToolOverrideGuard {
    fn drop(&mut self) {
        *test_tool_override_slot()
            .lock()
            .expect("test tool override slot lock") = self.previous.take();
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn install_test_tool_overrides(overrides: TestToolOverrideMap) -> TestToolOverrideGuard {
    let mut guard = test_tool_override_slot()
        .lock()
        .expect("test tool override slot lock");
    let previous = guard.replace(overrides);
    TestToolOverrideGuard { previous }
}

#[cfg(any(test, feature = "test-utils"))]
fn current_test_tool_overrides() -> Option<TestToolOverrideMap> {
    test_tool_override_slot()
        .lock()
        .expect("test tool override slot lock")
        .clone()
}

pub fn secret_resolver_from_storage(storage: &Storage) -> SecretResolver {
    let secrets = storage.secrets.clone();
    Arc::new(move |key| secrets.get_secret(key).ok().flatten())
}

fn wants_named_tool(tool_names: &[String], tool_name: &str) -> bool {
    tool_names.iter().any(|name| name == tool_name)
}

/// Default tools for main agents.
pub fn main_agent_default_tool_names() -> Vec<String> {
    vec![
        "bash",
        "file",
        "edit",
        "multiedit",
        "patch",
        "glob",
        "grep",
        "load_skill",
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
    _secret_resolver: Option<SecretResolver>,
    storage: Option<&Storage>,
    agent_id: Option<&str>,
    bash_config: Option<BashConfig>,
    workspace_root: Option<&Path>,
) -> anyhow::Result<ToolRegistry> {
    registry_from_allowlist_with_security_gate(
        tool_names,
        subagent_manager,
        _secret_resolver,
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
    _secret_resolver: Option<SecretResolver>,
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

    let wants_manage_agents = wants_named_tool(tool_names, "manage_agents");
    let wants_manage_tasks = wants_named_tool(tool_names, MANAGE_TASKS_TOOL_NAME);
    let wants_manage_task_tools = wants_manage_tasks;
    let wants_spawn_subagent = tool_names
        .iter()
        .any(|name| name == "spawn_subagent" || name == "spawn_subagent_batch");
    let wants_wait_subagents = tool_names.iter().any(|name| name == "wait_subagents");
    let wants_list_subagents = tool_names.iter().any(|name| name == "list_subagents");
    let wants_guarded_assessor =
        wants_manage_agents || wants_manage_task_tools || wants_spawn_subagent;
    let shared_assessor =
        storage.and_then(|value| wants_guarded_assessor.then(|| build_runtime_assessor(value)));
    let agent_crud_components = storage.and_then(|value| {
        wants_manage_agents.then(|| {
            build_agent_crud_components(
                value.agents.clone(),
                value.secrets.clone(),
                value.tasks.clone(),
            )
        })
    });
    let task_components = storage.and_then(|value| {
        wants_manage_task_tools
            .then(|| build_task_store_runtime_components(value, shared_assessor.clone()))
    });

    let mut builder = ToolRegistryBuilder::new();
    let mut allow_file = false;
    let mut allow_file_write = false;
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
            "spawn_subagent" | "spawn_subagent_batch" | "wait_subagents" | "list_subagents" => {}
            "load_skill" => {
                let provider = composite_skill_provider(storage);
                builder = if let Some(gate) = security_gate.clone() {
                    builder.with_load_skill_with_security(
                        provider,
                        gate,
                        agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                        DEFAULT_SECURITY_TASK_ID,
                    )
                } else {
                    builder.with_load_skill(provider)
                };
            }
            "run_skill" => {
                let mut tool =
                    RunSkillTool::new().with_root(crate::services::skills::skill_catalog_root()?);
                if let Some(gate) = security_gate.clone() {
                    tool = tool.with_security(
                        gate,
                        agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                        DEFAULT_SECURITY_TASK_ID,
                    );
                }
                builder.registry.register(tool);
            }

            // --- Storage-backed tools ---
            tool_name
                if is_task_management_tool_name(tool_name) || tool_name == "manage_agents" => {}
            "manage_marketplace" => {
                if storage.is_some() {
                    let registry_defaults = effective_config
                        .as_ref()
                        .map(|config| config.registry_defaults.clone())
                        .unwrap_or_default();
                    builder = builder.with_marketplace(Arc::new(
                        MarketplaceStoreAdapter::new_with_defaults(registry_defaults),
                    ));
                } else {
                    warn!(
                        tool_name = "manage_marketplace",
                        "Storage unavailable, skipping"
                    );
                }
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
                    builder.with_ops(Arc::new(OpsProviderAdapter::new(s.tasks.clone())))
                });
            }
            "skill" => {
                let provider = composite_skill_provider(storage);
                builder = if let Some(gate) = security_gate.clone() {
                    builder.with_skill_tool_with_security(
                        provider,
                        gate,
                        agent_id.unwrap_or(DEFAULT_SECURITY_AGENT_ID),
                        DEFAULT_SECURITY_TASK_ID,
                    )
                } else {
                    builder.with_skill_tool(provider)
                };
            }
            "memory_search" => {
                with_storage!(storage, "memory_search", builder, |s| {
                    let engine = UnifiedSearchEngine::new(
                        s.memory.clone(),
                        crate::services::session::SessionService::from_storage(s),
                    );
                    builder.with_unified_search(Arc::new(UnifiedMemorySearchAdapter::new(engine)))
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
                        s.tasks.clone(),
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
            // --- Search tools ---
            "glob" => {
                builder = builder.with_glob_and_base_dir(workspace_root.map(Path::to_path_buf));
            }
            "grep" => {
                builder = builder.with_grep_and_base_dir(workspace_root.map(Path::to_path_buf));
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
                let provider = composite_skill_provider(storage);
                if provider.get_skill(unknown).is_some() {
                    debug!(
                        skill_id = %unknown,
                        "Configured skill is loadable through load_skill; skipping standalone tool registration"
                    );
                    continue;
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

    if wants_manage_agents || wants_manage_task_tools {
        if storage.is_some() {
            builder = register_management_tools(
                builder,
                agent_crud_components
                    .as_ref()
                    .map(|components| components.store.clone()),
                task_components
                    .as_ref()
                    .map(|components| components.store.clone()),
                shared_assessor.clone(),
            );
        } else {
            if wants_manage_agents {
                debug!(tool_name = "manage_agents", "Storage missing, skipping");
            }
            if wants_manage_tasks {
                debug!(tool_name = "manage_tasks", "Storage missing, skipping");
            }
        }
    }

    // Check if batch tool was requested
    let wants_batch = tool_names.iter().any(|n| n == "batch");

    let mut registry = builder.build();

    if wants_spawn_subagent || wants_wait_subagents || wants_list_subagents {
        if let Some(manager) = &subagent_manager {
            register_subagent_management_tools(
                &mut registry,
                manager.clone(),
                if wants_spawn_subagent {
                    shared_assessor.clone()
                } else {
                    None
                },
            );
        } else {
            if wants_spawn_subagent {
                debug!(
                    tool_name = "spawn_subagent",
                    "Subagent manager missing, skipping"
                );
            }
            if wants_wait_subagents {
                debug!(
                    tool_name = "wait_subagents",
                    "Subagent manager missing, skipping"
                );
            }
            if wants_list_subagents {
                debug!(
                    tool_name = "list_subagents",
                    "Subagent manager missing, skipping"
                );
            }
        }
    }

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

    #[cfg(any(test, feature = "test-utils"))]
    if let Some(overrides) = current_test_tool_overrides() {
        for (_name, tool) in overrides {
            registry.register_arc(tool);
        }
    }

    // Populate known_tools for AgentStoreAdapter validation
    if let Some(agent_components) = &agent_crud_components {
        populate_known_tools_from_registry(&agent_components.known_tools, &registry);
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::{
        effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
    };
    use crate::models::AgentNode;
    use crate::prompt_files;
    use crate::storage::Storage;
    use crate::test_support::RestflowTestEnv;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_main_agent_default_tools_keep_management_out_of_default_surface() {
        let names = main_agent_default_tool_names();
        assert_eq!(
            names,
            vec![
                "bash",
                "file",
                "edit",
                "multiedit",
                "patch",
                "glob",
                "grep",
                "load_skill",
            ]
        );
        assert!(!names.contains(&"http_request".to_string()));
        assert!(!names.contains(&"send_email".to_string()));
        assert!(!names.contains(&"telegram_send".to_string()));
        assert!(!names.contains(&"discord_send".to_string()));
        assert!(!names.contains(&"slack_send".to_string()));
        assert!(!names.contains(&"http".to_string()));
        assert!(!names.contains(&"email".to_string()));
        assert!(!names.contains(&"telegram".to_string()));
        assert!(!names.contains(&"discord".to_string()));
        assert!(!names.contains(&"slack".to_string()));
        assert!(!names.contains(&"skill".to_string()));
        assert!(!names.contains(&"manage_tasks".to_string()));
        assert!(!names.contains(&"manage_agents".to_string()));
        assert!(!names.contains(&"manage_sessions".to_string()));
        assert!(!names.contains(&"manage_marketplace".to_string()));
        assert!(!names.contains(&"manage_triggers".to_string()));
        assert!(!names.contains(&"manage_ops".to_string()));
        assert!(!names.contains(&"manage_memory".to_string()));
        assert!(!names.contains(&"manage_auth_profiles".to_string()));
        assert!(!names.contains(&"manage_config".to_string()));
        assert!(!names.contains(&"manage_secrets".to_string()));
        assert!(!names.contains(&"task_list".to_string()));
        assert!(!names.contains(&"manage_tasks".to_string()));
    }

    #[test]
    fn test_manage_tasks_tool_registered_with_storage() {
        let state = RestflowTestEnv::new();
        let db_path = state.db_path("registry-tools.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let names = vec!["manage_tasks".to_string(), "manage_agents".to_string()];

        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None, None)
                .unwrap();
        assert!(registry.has("manage_tasks"));
        assert!(registry.has("manage_agents"));
    }

    #[test]
    fn test_manage_tasks_tool_skipped_without_storage() {
        let names = vec!["manage_tasks".to_string(), "manage_agents".to_string()];
        let registry =
            registry_from_allowlist(Some(&names), None, None, None, None, None, None).unwrap();
        assert!(!registry.has("manage_tasks"));
        assert!(!registry.has("manage_agents"));
    }

    #[test]
    fn test_unknown_skill_names_are_not_registered_as_runtime_tools() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-skills.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");

        let base_allowlist = vec!["load_skill".to_string()];
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
            "unknown skill-like names must not be auto-registered"
        );
        assert!(!registry.has("beta-skill"));

        let scoped_allowlist = vec![
            "load_skill".to_string(),
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
        assert!(
            !scoped_registry.has("alpha-skill"),
            "RestFlow no longer registers storage-owned skills as runtime tools"
        );
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
            "manage_terminal".to_string(),
            "manage_ops".to_string(),
            "security_query".to_string(),
        ];

        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None, None)
                .unwrap();
        assert!(registry.has("manage_marketplace"));
        assert!(registry.has("manage_terminal"));
        assert!(registry.has("manage_ops"));
        assert!(registry.has("security_query"));
    }

    #[test]
    fn test_main_agent_default_tools_exclude_external_and_management_tools() {
        let tools = main_agent_default_tool_names();
        assert!(!tools.iter().any(|name| name == "run_skill"));
        assert!(!tools.iter().any(|name| name == "python"));
        assert!(!tools.iter().any(|name| name == "browser"));
        assert!(!tools.iter().any(|name| name == "transcribe"));
        assert!(!tools.iter().any(|name| name == "vision"));
        assert!(!tools.iter().any(|name| name == "switch_model"));
        assert!(!tools.iter().any(|name| name == "manage_terminal"));
        assert!(!tools.iter().any(|name| name == "security_query"));
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
    fn test_registry_from_allowlist_uses_configured_diagnostics_defaults() {
        let env = RestflowTestEnv::new();
        let db_path = env.db_path("registry-api-defaults.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let mut config = storage
            .config
            .get_config()
            .expect("config should load")
            .expect("config should exist");
        config.api_defaults.diagnostics_timeout_ms = 9_000;
        storage
            .config
            .update_config(config)
            .expect("config should update");

        let names = vec!["diagnostics".to_string()];
        let registry = registry_from_allowlist(
            Some(&names),
            None,
            None,
            Some(&storage),
            None,
            None,
            Some(env.root()),
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_manage_tasks_runtime_registry_injects_store_assessor() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-bg-runtime.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let agent_id = storage
            .agents
            .create_agent("Runtime Owner".to_string(), AgentNode::default())
            .expect("agent should be created")
            .id;

        let names = vec!["manage_tasks".to_string()];
        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None, None)
                .expect("registry should be built");
        let output = registry
            .get("manage_tasks")
            .expect("manage_tasks should be registered")
            .execute(json!({
                "operation": "create",
                "name": "Runtime Preview Task",
                "agent_id": agent_id,
                "input": "Run checks",
                "schedule": {
                    "type": "interval",
                    "interval_ms": 60000,
                    "start_at": null
                },
                "preview": true
            }))
            .await
            .expect("runtime tool should not fail when store assessor is injected");

        assert!(output.success);
        assert_eq!(output.result["status"], "preview");
    }

    #[tokio::test]
    async fn test_manage_agents_runtime_registry_injects_shared_assessor() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("registry-agent-runtime.db");
        let storage = Storage::new(db_path.to_str().expect("db path should be valid"))
            .expect("storage should be created");
        let prompts_dir = dir.path().join("agents");
        std::fs::create_dir_all(&prompts_dir).expect("prompts dir should be created");

        let previous_agents_dir = std::env::var_os(prompt_files::AGENTS_DIR_ENV);
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, &prompts_dir) };

        let names = vec![
            "manage_agents".to_string(),
            "manage_tasks".to_string(),
            "bash".to_string(),
            "file".to_string(),
        ];
        let registry =
            registry_from_allowlist(Some(&names), None, None, Some(&storage), None, None, None)
                .expect("registry should be built");

        unsafe {
            match previous_agents_dir {
                Some(value) => std::env::set_var(prompt_files::AGENTS_DIR_ENV, value),
                None => std::env::remove_var(prompt_files::AGENTS_DIR_ENV),
            }
        }

        let output = registry
            .get("manage_agents")
            .expect("manage_agents should be registered")
            .execute(json!({
                "operation": "create",
                "name": "Runtime Preview Agent",
                "agent": {
                    "tools": ["bash", "file", "manage_tasks"]
                },
                "preview": true
            }))
            .await
            .expect("runtime tool should not fail when assessor is injected");

        assert!(output.success);
        assert_eq!(output.result["status"], "preview");
    }
}
