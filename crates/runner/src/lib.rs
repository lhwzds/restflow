//! RestFlow session runner and agent execution orchestration.

pub use restflow_core::*;
pub use tools;

pub mod runtime {
    pub mod agent {
        //! Agent execution engine components.
        //!
        //! Ownership rule:
        //! - `runner::runtime::agent` exposes tool assembly and prompt helpers.
        //! - AI-owned subagent runtime types stay in `agent` / `types`.
        //! - Do not re-export `SubagentManagerImpl`, `SubagentDeps`, or related runtime
        //!   state from this module.

        pub mod tools {
            //! Unified tool registry for agent execution.
            //!
            //! Tool implementations live in `tools`. This module provides
            //! assembly functions (`registry_from_allowlist`) that combine tools with
            //! storage-backed services from `runtime`.

            pub(crate) mod assembly {
                use std::collections::HashSet;
                use std::sync::{Arc, RwLock};

                use super::SUBAGENT_TOOL_NAMES;
                use crate::AgentStorage;
                use crate::services::adapters::AgentStoreAdapter;
                use crate::storage::SecretStorage;
                use crate::tools::{
                    BashConfig, FileConfig, ListSubagentsTool, SpawnSubagentBatchTool,
                    SpawnSubagentTool, ToolRegistryBuilder, WaitSubagentsTool,
                };
                use types::SubagentManager;
                use types::store::AgentStore;
                use types::tool::SecurityGate;
                use types::toolset::ToolRegistry;

                pub(crate) struct AgentCrudComponents {
                    pub known_tools: Arc<RwLock<HashSet<String>>>,
                    pub store: Arc<dyn AgentStore>,
                }

                pub(crate) fn register_bash_execution_tool(
                    mut builder: ToolRegistryBuilder,
                    config: BashConfig,
                    security_gate: Option<Arc<dyn SecurityGate>>,
                    agent_id: &str,
                    task_id: &str,
                ) -> ToolRegistryBuilder {
                    if let Some(gate) = security_gate {
                        builder.registry.register(
                            config
                                .into_bash_tool()
                                .with_security(gate, agent_id, task_id),
                        );
                    } else {
                        builder = builder.with_bash(config);
                    }
                    builder
                }

                pub(crate) fn register_file_execution_tool(
                    mut builder: ToolRegistryBuilder,
                    config: FileConfig,
                    security_gate: Option<Arc<dyn SecurityGate>>,
                    agent_id: &str,
                    task_id: &str,
                ) -> ToolRegistryBuilder {
                    if let Some(gate) = security_gate.clone() {
                        let tool = config
                            .into_file_tool_with_tracker(builder.tracker())
                            .with_security(gate, agent_id, task_id);
                        builder.registry.register(tool);
                    } else {
                        builder = builder.with_file(config);
                    }

                    builder
                }

                pub(crate) fn populate_known_tools_from_registry(
                    known_tools: &Arc<RwLock<HashSet<String>>>,
                    registry: &ToolRegistry,
                ) {
                    if let Ok(mut known) = known_tools.write() {
                        *known = registry
                            .list()
                            .into_iter()
                            .map(|name| name.to_string())
                            .collect::<HashSet<_>>();
                        for name in [
                            "bash",
                            "file",
                            "edit",
                            "multiedit",
                            "patch",
                            "glob",
                            "grep",
                            "load_skill",
                            "run_skill",
                            "manage_agents",
                        ] {
                            known.insert(name.to_string());
                        }
                        for name in SUBAGENT_TOOL_NAMES {
                            known.insert((*name).to_string());
                        }
                    }
                }

                pub(crate) fn build_agent_crud_components(
                    agent_storage: AgentStorage,
                    secret_storage: SecretStorage,
                ) -> AgentCrudComponents {
                    let known_tools = Arc::new(RwLock::new(HashSet::new()));
                    let store: Arc<dyn AgentStore> = Arc::new(AgentStoreAdapter::new(
                        agent_storage,
                        secret_storage,
                        known_tools.clone(),
                    ));
                    AgentCrudComponents { known_tools, store }
                }

                pub(crate) fn register_management_tools(
                    mut builder: ToolRegistryBuilder,
                    agent_store: Option<Arc<dyn AgentStore>>,
                ) -> ToolRegistryBuilder {
                    if let Some(agent_store) = agent_store {
                        builder = builder.with_agent_crud(agent_store);
                    }

                    builder
                }

                pub(crate) fn register_subagent_management_tools(
                    registry: &mut ToolRegistry,
                    manager: Arc<dyn SubagentManager>,
                ) {
                    registry.register(SpawnSubagentTool::new(manager.clone()));
                    registry.register(SpawnSubagentBatchTool::new(manager.clone()));
                    registry.register(WaitSubagentsTool::new(manager.clone()));
                    registry.register(ListSubagentsTool::new(manager));
                }
            }
            pub mod skill_activation {
                //! Skill-aware tool allowlist activation helpers.

                use crate::services::skill_mentions::parse_skill_mentions;
                use anyhow::{Result, bail};
                use std::collections::{HashMap, HashSet};
                use types::{AgentNode, Skill};

                use super::effective_main_agent_tool_names;

                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum SkillActivationPolicy {
                    Strict,
                    IgnoreInvalid,
                }

                #[derive(Debug, Clone, PartialEq, Eq)]
                pub struct SkillActivationResult {
                    pub tool_names: Vec<String>,
                    pub activated_skill_ids: Vec<String>,
                    pub issues: Vec<SkillActivationIssue>,
                }

                #[derive(Debug, Clone, PartialEq, Eq)]
                pub struct SkillActivationIssue {
                    pub category: SkillActivationIssueCategory,
                    pub skill_id: String,
                    pub message: String,
                    pub suggestion: Option<String>,
                }

                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum SkillActivationIssueCategory {
                    MissingSkill,
                    UnauthorizedSkill,
                }

                impl SkillActivationIssueCategory {
                    pub fn as_str(self) -> &'static str {
                        match self {
                            Self::MissingSkill => "missing_skill",
                            Self::UnauthorizedSkill => "unauthorized_skill",
                        }
                    }
                }

                pub fn effective_tool_allowlist_for_turn(
                    agent_node: &AgentNode,
                    user_input: Option<&str>,
                    skill_catalog: &[Skill],
                    policy: SkillActivationPolicy,
                ) -> Result<SkillActivationResult> {
                    let base_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
                    resolve_skill_activated_tool_allowlist(
                        &base_tools,
                        agent_node.skills.as_deref(),
                        user_input,
                        skill_catalog,
                        policy,
                    )
                }

                pub fn resolve_skill_activated_tool_allowlist(
                    base_tool_names: &[String],
                    assigned_skill_ids: Option<&[String]>,
                    user_input: Option<&str>,
                    skill_catalog: &[Skill],
                    policy: SkillActivationPolicy,
                ) -> Result<SkillActivationResult> {
                    let mut result = SkillActivationResult {
                        tool_names: dedupe_strings(base_tool_names.iter().cloned()),
                        activated_skill_ids: Vec::new(),
                        issues: Vec::new(),
                    };
                    let mut tool_set: HashSet<String> = result.tool_names.iter().cloned().collect();
                    let mut activated_set = HashSet::new();
                    let skill_by_id = build_effective_skill_index(skill_catalog);
                    let assigned_ids = assigned_skill_ids.unwrap_or_default();

                    for skill_id in assigned_ids {
                        activate_assigned_skill(
                            skill_id,
                            &skill_by_id,
                            &mut result,
                            &mut tool_set,
                            &mut activated_set,
                        );
                    }

                    let mentioned_ids = user_input.map(parse_skill_mentions).unwrap_or_default();
                    for skill_id in mentioned_ids {
                        activate_mentioned_skill(
                            &skill_id,
                            assigned_ids,
                            &skill_by_id,
                            &mut result,
                            &mut tool_set,
                            &mut activated_set,
                        );
                    }

                    if policy == SkillActivationPolicy::Strict && !result.issues.is_empty() {
                        bail!(format_skill_activation_issues(&result.issues));
                    }

                    Ok(result)
                }

                fn activate_assigned_skill(
                    skill_id: &str,
                    skill_by_id: &HashMap<&str, &Skill>,
                    result: &mut SkillActivationResult,
                    tool_set: &mut HashSet<String>,
                    activated_set: &mut HashSet<String>,
                ) {
                    let Some(skill) = skill_by_id.get(skill_id).copied() else {
                        result.issues.push(SkillActivationIssue {
                            category: SkillActivationIssueCategory::MissingSkill,
                            skill_id: skill_id.to_string(),
                            message: format!("Assigned skill '{}' was not found", skill_id),
                            suggestion: Some(
                                "Remove the skill from agent.skills or install it".to_string(),
                            ),
                        });
                        return;
                    };

                    add_skill_suggested_tools(skill, result, tool_set, activated_set);
                }

                fn activate_mentioned_skill(
                    skill_id: &str,
                    assigned_skill_ids: &[String],
                    skill_by_id: &HashMap<&str, &Skill>,
                    result: &mut SkillActivationResult,
                    tool_set: &mut HashSet<String>,
                    activated_set: &mut HashSet<String>,
                ) {
                    add_tool("load_skill", result, tool_set);

                    let Some(skill) = skill_by_id.get(skill_id).copied() else {
                        result.issues.push(SkillActivationIssue {
                            category: SkillActivationIssueCategory::MissingSkill,
                            skill_id: skill_id.to_string(),
                            message: format!("Mentioned skill '{}' was not found", skill_id),
                            suggestion: Some("Install the skill or remove the mention".to_string()),
                        });
                        return;
                    };

                    if !assigned_skill_ids.iter().any(|id| id == skill_id) {
                        result.issues.push(SkillActivationIssue {
                            category: SkillActivationIssueCategory::UnauthorizedSkill,
                            skill_id: skill_id.to_string(),
                            message: format!(
                                "Mentioned skill '{}' is not assigned to this agent",
                                skill_id
                            ),
                            suggestion: Some(
                                "Add the skill to agent.skills before using it".to_string(),
                            ),
                        });
                        return;
                    }

                    add_skill_suggested_tools(skill, result, tool_set, activated_set);
                }

                fn add_skill_suggested_tools(
                    skill: &Skill,
                    result: &mut SkillActivationResult,
                    tool_set: &mut HashSet<String>,
                    activated_set: &mut HashSet<String>,
                ) {
                    if activated_set.insert(skill.id.clone()) {
                        result.activated_skill_ids.push(skill.id.clone());
                    }
                    for tool_name in &skill.suggested_tools {
                        add_tool(tool_name, result, tool_set);
                    }
                }

                fn add_tool(
                    tool_name: &str,
                    result: &mut SkillActivationResult,
                    tool_set: &mut HashSet<String>,
                ) {
                    if tool_set.insert(tool_name.to_string()) {
                        result.tool_names.push(tool_name.to_string());
                    }
                }

                fn build_effective_skill_index(skills: &[Skill]) -> HashMap<&str, &Skill> {
                    let mut skill_by_id = HashMap::new();
                    for skill in skills {
                        skill_by_id.entry(skill.id.as_str()).or_insert(skill);
                    }
                    skill_by_id
                }

                fn dedupe_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
                    let mut seen = HashSet::new();
                    let mut deduped = Vec::new();
                    for value in values {
                        if seen.insert(value.clone()) {
                            deduped.push(value);
                        }
                    }
                    deduped
                }

                fn format_skill_activation_issues(issues: &[SkillActivationIssue]) -> String {
                    let messages = issues
                        .iter()
                        .map(|issue| issue.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!("Skill activation failed: {}", messages)
                }

                #[cfg(test)]
                mod tests {
                    use super::*;
                    use types::SkillSource;

                    fn skill(id: &str, suggested_tools: &[&str]) -> Skill {
                        let mut skill = Skill::new(
                            id.to_string(),
                            id.to_string(),
                            None,
                            None,
                            "content".to_string(),
                        );
                        skill.suggested_tools = suggested_tools
                            .iter()
                            .map(|tool| (*tool).to_string())
                            .collect();
                        skill
                    }

                    #[test]
                    fn assigned_skill_adds_suggested_tools() {
                        let base_tools = vec!["bash".to_string()];
                        let assigned = vec!["review".to_string()];
                        let catalog = vec![skill("review", &["grep", "file"])];

                        let result = resolve_skill_activated_tool_allowlist(
                            &base_tools,
                            Some(&assigned),
                            None,
                            &catalog,
                            SkillActivationPolicy::Strict,
                        )
                        .expect("activation should succeed");

                        assert_eq!(
                            result.tool_names,
                            vec!["bash".to_string(), "grep".to_string(), "file".to_string()]
                        );
                        assert_eq!(result.activated_skill_ids, vec!["review".to_string()]);
                        assert!(result.issues.is_empty());
                    }

                    #[test]
                    fn explicit_mention_adds_load_skill_and_suggested_tools() {
                        let base_tools = vec!["bash".to_string()];
                        let assigned = vec!["team".to_string()];
                        let catalog = vec![skill("team", &["spawn_subagent_batch"])];

                        let result = resolve_skill_activated_tool_allowlist(
                            &base_tools,
                            Some(&assigned),
                            Some("Use @team for this task"),
                            &catalog,
                            SkillActivationPolicy::Strict,
                        )
                        .expect("activation should succeed");

                        assert!(result.tool_names.contains(&"load_skill".to_string()));
                        assert!(
                            result
                                .tool_names
                                .contains(&"spawn_subagent_batch".to_string())
                        );
                        assert_eq!(result.activated_skill_ids, vec!["team".to_string()]);
                    }

                    #[test]
                    fn known_unassigned_mention_reports_unauthorized_without_adding_tools() {
                        let base_tools = vec!["bash".to_string()];
                        let assigned = vec!["regular".to_string()];
                        let catalog =
                            vec![skill("admin", &["manage_secrets"]), skill("regular", &[])];

                        let result = resolve_skill_activated_tool_allowlist(
                            &base_tools,
                            Some(&assigned),
                            Some("@admin rotate credentials"),
                            &catalog,
                            SkillActivationPolicy::IgnoreInvalid,
                        )
                        .expect("ignore invalid policy should return issues");

                        assert!(result.tool_names.contains(&"load_skill".to_string()));
                        assert!(!result.tool_names.contains(&"manage_secrets".to_string()));
                        assert_eq!(result.issues.len(), 1);
                        assert_eq!(
                            result.issues[0].category,
                            SkillActivationIssueCategory::UnauthorizedSkill
                        );
                    }

                    #[test]
                    fn missing_mention_reports_issue_without_adding_tools() {
                        let base_tools = vec!["bash".to_string()];
                        let catalog = vec![skill("regular", &[])];

                        let result = resolve_skill_activated_tool_allowlist(
                            &base_tools,
                            None,
                            Some("@missing do work"),
                            &catalog,
                            SkillActivationPolicy::IgnoreInvalid,
                        )
                        .expect("ignore invalid policy should return issues");

                        assert!(result.tool_names.contains(&"load_skill".to_string()));
                        assert!(!result.tool_names.contains(&"manage_secrets".to_string()));
                        assert_eq!(result.issues.len(), 1);
                        assert_eq!(
                            result.issues[0].category,
                            SkillActivationIssueCategory::MissingSkill
                        );
                    }

                    #[test]
                    fn strict_policy_rejects_missing_assigned_skill() {
                        let base_tools = vec!["bash".to_string()];
                        let assigned = vec!["missing".to_string()];

                        let error = resolve_skill_activated_tool_allowlist(
                            &base_tools,
                            Some(&assigned),
                            None,
                            &[],
                            SkillActivationPolicy::Strict,
                        )
                        .expect_err("strict policy should fail");

                        assert!(
                            error
                                .to_string()
                                .contains("Assigned skill 'missing' was not found")
                        );
                    }

                    #[test]
                    fn first_catalog_entry_wins_for_shadowed_read_only_skill() {
                        let mut system_skill = skill("team", &["spawn_subagent_batch"]);
                        system_skill.source = SkillSource::External;
                        let mut storage_skill = skill("team", &["manage_secrets"]);
                        storage_skill.source = SkillSource::User;
                        let base_tools = vec!["bash".to_string()];
                        let assigned = vec!["team".to_string()];

                        let result = resolve_skill_activated_tool_allowlist(
                            &base_tools,
                            Some(&assigned),
                            None,
                            &[system_skill, storage_skill],
                            SkillActivationPolicy::Strict,
                        )
                        .expect("read-only skill should win");

                        assert!(
                            result
                                .tool_names
                                .contains(&"spawn_subagent_batch".to_string())
                        );
                        assert!(!result.tool_names.contains(&"manage_secrets".to_string()));
                    }

                    #[test]
                    fn agent_wrapper_uses_effective_main_agent_tools() {
                        let agent = AgentNode::new().with_skills(vec!["review".to_string()]);
                        let catalog = vec![skill("review", &["grep"])];

                        let result = effective_tool_allowlist_for_turn(
                            &agent,
                            None,
                            &catalog,
                            SkillActivationPolicy::Strict,
                        )
                        .expect("activation should succeed");

                        assert!(result.tool_names.contains(&"bash".to_string()));
                        assert!(result.tool_names.contains(&"grep".to_string()));
                    }
                }
            }

            use std::path::Path;
            use std::sync::Arc;
            #[cfg(any(test, feature = "test-utils"))]
            use std::sync::{Mutex, OnceLock};
            use tracing::{debug, warn};

            use self::assembly::{
                build_agent_crud_components, populate_known_tools_from_registry,
                register_bash_execution_tool, register_file_execution_tool,
                register_management_tools, register_subagent_management_tools,
            };
            use crate::services::adapters::*;
            use crate::storage::Storage;
            use types::SubagentManager;
            use types::skill::SkillProvider;
            use types::tool::SecurityGate;

            // Re-export tool types from tools
            pub use crate::tools::impls::{
                BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
                RunSkillTool, SpawnSubagentTool, ToolRegistryBuilder, WaitSubagentsTool,
                default_registry,
            };

            pub use ::agent::tools::{SecretResolver, Tool, ToolOutput, ToolRegistry};
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
            pub fn install_test_tool_overrides(
                overrides: TestToolOverrideMap,
            ) -> TestToolOverrideGuard {
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
                    "run_skill",
                    "spawn_subagent",
                    "spawn_subagent_batch",
                    "wait_subagents",
                    "list_subagents",
                ]
                .into_iter()
                .map(str::to_string)
                .collect()
            }

            pub const SUBAGENT_TOOL_NAMES: &[&str] = &[
                "spawn_subagent",
                "spawn_subagent_batch",
                "wait_subagents",
                "list_subagents",
            ];

            pub fn is_subagent_tool_name(name: &str) -> bool {
                SUBAGENT_TOOL_NAMES.contains(&name)
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
                let wants_spawn_subagent = tool_names
                    .iter()
                    .any(|name| name == "spawn_subagent" || name == "spawn_subagent_batch");
                let wants_wait_subagents = tool_names.iter().any(|name| name == "wait_subagents");
                let wants_list_subagents = tool_names.iter().any(|name| name == "list_subagents");
                let agent_crud_components = storage.and_then(|value| {
                    wants_manage_agents.then(|| {
                        build_agent_crud_components(value.agents.clone(), value.secrets.clone())
                    })
                });

                let mut builder = ToolRegistryBuilder::new();
                let mut allow_file = false;
                let mut allow_file_write = false;
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
                            let mut config = bash_config.clone().unwrap_or_default();
                            if config.working_dir.is_none() {
                                config.working_dir =
                                    workspace_root.map(|path| path.to_string_lossy().into_owned());
                            }
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
                        "patch" => {
                            builder = builder
                                .with_patch_and_base_dir(workspace_root.map(Path::to_path_buf));
                        }
                        "edit" => {
                            builder = builder
                                .with_edit_and_base_dir(workspace_root.map(Path::to_path_buf));
                        }
                        "multiedit" => {
                            builder = builder
                                .with_multiedit_and_base_dir(workspace_root.map(Path::to_path_buf));
                        }

                        // --- Subagent tools ---
                        "spawn_subagent"
                        | "spawn_subagent_batch"
                        | "wait_subagents"
                        | "list_subagents" => {}
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
                            let mut tool = RunSkillTool::new()
                                .with_root(crate::services::skills::skill_catalog_root()?);
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
                        tool_name if tool_name == "manage_agents" => {}
                        "manage_ops" => {
                            builder = builder.with_ops(Arc::new(OpsProviderAdapter::new()));
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
                                    s.file_sessions.clone(),
                                    s.agents.clone(),
                                )))
                            });
                        }
                        // --- Search tools ---
                        "glob" => {
                            builder = builder
                                .with_glob_and_base_dir(workspace_root.map(Path::to_path_buf));
                        }
                        "grep" => {
                            builder = builder
                                .with_grep_and_base_dir(workspace_root.map(Path::to_path_buf));
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

                if wants_manage_agents {
                    if storage.is_some() {
                        builder = register_management_tools(
                            builder,
                            agent_crud_components
                                .as_ref()
                                .map(|components| components.store.clone()),
                        );
                    } else {
                        if wants_manage_agents {
                            debug!(tool_name = "manage_agents", "Storage missing, skipping");
                        }
                    }
                }

                // Check if batch tool was requested
                let wants_batch = tool_names.iter().any(|n| n == "batch");

                let mut registry = builder.build();

                if wants_spawn_subagent || wants_wait_subagents || wants_list_subagents {
                    if let Some(manager) = &subagent_manager {
                        register_subagent_management_tools(&mut registry, manager.clone());
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
                    registry.register(crate::tools::BatchTool::new(registry_arc));
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
                    effective_main_agent_tool_names, main_agent_default_tool_names,
                    registry_from_allowlist,
                };
                use crate::storage::Storage;
                use tempfile::tempdir;

                #[test]
                fn test_main_agent_default_tools_keep_narrow_management_surface() {
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
                            "run_skill",
                            "spawn_subagent",
                            "spawn_subagent_batch",
                            "wait_subagents",
                            "list_subagents",
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
                    assert!(!names.contains(&"manage_agents".to_string()));
                    assert!(!names.contains(&"manage_sessions".to_string()));
                    assert!(!names.contains(&"manage_triggers".to_string()));
                    assert!(!names.contains(&"manage_ops".to_string()));
                    assert!(!names.contains(&"manage_memory".to_string()));
                    assert!(!names.contains(&"manage_config".to_string()));
                    assert!(!names.contains(&"manage_secrets".to_string()));
                    assert!(!names.contains(&"task_list".to_string()));
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
                    let names = vec!["manage_ops".to_string()];

                    let registry = registry_from_allowlist(
                        Some(&names),
                        None,
                        None,
                        Some(&storage),
                        None,
                        None,
                        None,
                    )
                    .unwrap();
                    assert!(registry.has("manage_ops"));
                }

                #[test]
                fn test_main_agent_default_tools_exclude_external_and_management_tools() {
                    let tools = main_agent_default_tool_names();
                    assert!(tools.iter().any(|name| name == "load_skill"));
                    assert!(tools.iter().any(|name| name == "run_skill"));
                    assert!(!tools.iter().any(|name| name == "python"));
                    assert!(!tools.iter().any(|name| name == "browser"));
                    assert!(!tools.iter().any(|name| name == "transcribe"));
                    assert!(!tools.iter().any(|name| name == "vision"));
                    assert!(!tools.iter().any(|name| name == "switch_model"));
                }

                #[tokio::test]
                async fn test_filesystem_tools_require_workspace_root_when_unset() {
                    let names = vec!["file".to_string(), "glob".to_string(), "grep".to_string()];
                    let registry =
                        registry_from_allowlist(Some(&names), None, None, None, None, None, None)
                            .unwrap();

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

                #[tokio::test]
                async fn test_bash_uses_workspace_root_as_default_workdir() {
                    let dir = tempdir().expect("temp dir should be created");
                    let names = vec!["bash".to_string()];
                    let registry = registry_from_allowlist(
                        Some(&names),
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(dir.path()),
                    )
                    .unwrap();

                    let result = registry
                        .get("bash")
                        .unwrap()
                        .execute(serde_json::json!({
                            "command": "pwd"
                        }))
                        .await
                        .unwrap();

                    assert!(result.success, "{result:?}");
                    let expected_root =
                        std::fs::canonicalize(dir.path()).expect("temp dir should resolve");
                    let expected_stdout = format!("{}\n", expected_root.display());
                    assert_eq!(
                        result.result.get("stdout").and_then(|value| value.as_str()),
                        Some(expected_stdout.as_str())
                    );
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
        }

        use std::sync::Arc;
        use tracing::warn;

        use crate::storage::Storage;
        use ::agent::agent::DEFAULT_AGENT_PROMPT;
        use types::AgentNode;

        const DEFAULT_MAIN_AGENT_PROMPT: &str = r#"You are a RestFlow agent.

        RestFlow is being simplified into an agent framework with a small runtime core:
        agent execution, skill discovery, executable skill runs, and client surfaces such
        as the TUI. Keep the runtime focused on solving the current user request with
        the tools that are actually available.

        ## Default Tool Surface

        Use only the tools present in the current tool list. The minimal core toolset is:

        - `bash`: Run shell commands in the workspace when command execution is needed.
        - `file`: Read and write files through the file tool when available.
        - `edit`, `multiedit`, `patch`: Apply targeted code edits.
        - `glob`, `grep`: Search files and text.
        - `load_skill`: List or read skill guidance. This tool is load-only.
        - `run_skill`: Execute an installed `skrun` skill by ID with JSON input.

        Do not assume network, notification, browser, memory, marketplace, task
        management, Python execution, or provider-management tools are available unless
        they appear in the current tool list.

        ## Skill Rules

        - Use `load_skill` to inspect available skills before relying on specialized
          guidance.
        - Use `run_skill` only for installed executable `skrun` skills.
        - Do not try to execute skills through `load_skill`.
        - Treat external capabilities such as Python execution, HTTP calls, web search,
          browser automation, audio transcription, image analysis, and notifications as
          external `skrun` skills, not core runtime tools.

        ## Working Style

        - Prefer direct action over long explanation when the user's request is clear.
        - Keep changes small and targeted.
        - Read before editing.
        - Use structured edits for source changes.
        - Verify important changes with focused commands or tests.
        - Report blockers clearly when required tools, credentials, or permissions are
          unavailable.

        ## Safety

        - Do not invent tools.
        - Do not create durable tasks, agents, memories, secrets, or marketplace entries
          unless a matching management surface is explicitly available.
        - If a command or tool requires approval, wait for approval before retrying.
        "#;

        pub use tools::{
            BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
            SkillActivationPolicy, SpawnSubagentTool, Tool, ToolRegistry, ToolRegistryBuilder,
            ToolResult, WaitSubagentsTool, default_registry, effective_main_agent_tool_names,
            effective_tool_allowlist_for_turn, main_agent_default_tool_names,
            registry_from_allowlist, secret_resolver_from_storage,
        };
        #[cfg(any(test, feature = "test-utils"))]
        pub use tools::{TestToolOverrideGuard, install_test_tool_overrides};

        /// Build the agent system prompt from agent configuration.
        ///
        /// Skills are now registered as callable tools (via `registry_from_allowlist`),
        /// so they are no longer injected into the system prompt.
        pub fn build_agent_system_prompt(
            storage: Arc<Storage>,
            agent_node: &AgentNode,
            agent_id: Option<&str>,
        ) -> Result<String, anyhow::Error> {
            let base = agent_id
                .and_then(|id| match storage.agents.get_agent(id.to_string()) {
                    Ok(Some(stored_agent)) => stored_agent
                        .agent
                        .prompt
                        .filter(|prompt| !prompt.trim().is_empty()),
                    Ok(None) => None,
                    Err(err) => {
                        warn!(
                            agent_id = %id,
                            error = %err,
                            "Failed to load agent prompt from file; falling back"
                        );
                        None
                    }
                })
                .or_else(|| {
                    agent_node
                        .prompt
                        .clone()
                        .filter(|prompt| !prompt.trim().is_empty())
                })
                .or_else(|| Some(DEFAULT_MAIN_AGENT_PROMPT.to_string()))
                .unwrap_or_else(|| DEFAULT_AGENT_PROMPT.to_string());
            Ok(base)
        }
    }
    pub mod execution_context {
        //! Shared execution context metadata across main and sub-agent flows.

        use serde::{Deserialize, Serialize};

        /// High-level runtime role for an execution.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ExecutionRole {
            /// Foreground interactive chat turn.
            MainAgent,
            /// Child agent spawned by another agent.
            Subagent,
        }

        impl ExecutionRole {
            pub fn as_str(self) -> &'static str {
                match self {
                    Self::MainAgent => "main_agent",
                    Self::Subagent => "subagent",
                }
            }
        }

        /// Common context envelope used to describe an execution identity.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct ExecutionContext {
            pub role: ExecutionRole,
            pub agent_id: String,
            pub chat_session_id: Option<String>,
            pub parent_run_id: Option<String>,
        }

        impl ExecutionContext {
            pub fn main(agent_id: impl Into<String>, chat_session_id: impl Into<String>) -> Self {
                Self {
                    role: ExecutionRole::MainAgent,
                    agent_id: agent_id.into(),
                    chat_session_id: Some(chat_session_id.into()),
                    parent_run_id: None,
                }
            }

            pub fn subagent(agent_id: impl Into<String>, parent_run_id: impl Into<String>) -> Self {
                Self {
                    role: ExecutionRole::Subagent,
                    agent_id: agent_id.into(),
                    chat_session_id: None,
                    parent_run_id: Some(parent_run_id.into()),
                }
            }

            pub fn to_value(&self) -> serde_json::Value {
                serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn main_context_sets_session() {
                let context = ExecutionContext::main("agent-1", "session-1");
                assert_eq!(context.role, ExecutionRole::MainAgent);
                assert_eq!(context.chat_session_id.as_deref(), Some("session-1"));
            }

            #[test]
            fn subagent_context_sets_parent_run() {
                let context = ExecutionContext::subagent("agent-2", "exec-1");
                assert_eq!(context.role, ExecutionRole::Subagent);
                assert_eq!(context.parent_run_id.as_deref(), Some("exec-1"));
                assert!(context.chat_session_id.is_none());
            }

            #[test]
            fn subagent_context_serializes_parent_run_id() {
                let context = ExecutionContext::subagent("agent-2", "exec-1");
                let value = context.to_value();
                assert_eq!(value["parent_run_id"], "exec-1");
            }

            #[test]
            fn role_as_str_is_stable() {
                assert_eq!(ExecutionRole::MainAgent.as_str(), "main_agent");
                assert_eq!(ExecutionRole::Subagent.as_str(), "subagent");
            }

            #[test]
            fn context_serializes_to_json_value() {
                let context = ExecutionContext::main("agent-1", "session-1");
                let value = context.to_value();
                assert_eq!(value["role"], "main_agent");
                assert_eq!(value["agent_id"], "agent-1");
                assert_eq!(value["chat_session_id"], "session-1");
            }
        }
    }
    pub mod orchestrator {
        pub mod kernel {
            use std::sync::Arc;

            use anyhow::Result;
            use async_trait::async_trait;

            use crate::runtime::session_runner::{
                AgentRuntimeExecutor, SessionExecutionResult, SessionInputMode,
                SessionTurnRuntimeOptions,
            };
            use ::agent::agent::StreamEmitter;
            use types::ChatSession;
            use types::{ExecutionOutcome, ExecutionPlan};

            #[async_trait]
            pub trait ExecutionBackend: Send + Sync {
                fn load_chat_session(&self, session_id: &str) -> Result<ChatSession>;

                fn prepare_interactive_session(&self, _session: &mut ChatSession) -> Result<()> {
                    Ok(())
                }

                async fn execute_interactive_session_turn(
                    &self,
                    session: &mut ChatSession,
                    user_input: &str,
                    max_history: usize,
                    input_mode: SessionInputMode,
                    emitter: Option<Box<dyn StreamEmitter>>,
                    options: SessionTurnRuntimeOptions,
                ) -> Result<SessionExecutionResult>;

                async fn execute_subagent_plan(
                    &self,
                    plan: ExecutionPlan,
                ) -> Result<ExecutionOutcome>;
            }

            #[derive(Clone)]
            pub struct ExecutionKernel {
                backend: Arc<dyn ExecutionBackend>,
            }

            impl ExecutionKernel {
                pub fn new(backend: Arc<dyn ExecutionBackend>) -> Self {
                    Self { backend }
                }

                pub fn backend(&self) -> Arc<dyn ExecutionBackend> {
                    self.backend.clone()
                }
            }

            #[async_trait]
            impl ExecutionBackend for AgentRuntimeExecutor {
                fn load_chat_session(&self, session_id: &str) -> Result<ChatSession> {
                    self.load_chat_session(session_id)
                }

                fn prepare_interactive_session(&self, session: &mut ChatSession) -> Result<()> {
                    let _ = self.resolve_stored_agent_for_session(session)?;
                    Ok(())
                }

                async fn execute_interactive_session_turn(
                    &self,
                    session: &mut ChatSession,
                    user_input: &str,
                    max_history: usize,
                    input_mode: SessionInputMode,
                    emitter: Option<Box<dyn StreamEmitter>>,
                    options: SessionTurnRuntimeOptions,
                ) -> Result<SessionExecutionResult> {
                    self.execute_session_turn_with_emitter_and_steer(
                        session,
                        user_input,
                        max_history,
                        input_mode,
                        emitter,
                        options,
                    )
                    .await
                }

                async fn execute_subagent_plan(
                    &self,
                    plan: ExecutionPlan,
                ) -> Result<ExecutionOutcome> {
                    self.execute_subagent_plan(plan).await
                }
            }

            pub fn parse_optional_metadata<T: serde::de::DeserializeOwned>(
                plan: &types::ExecutionPlan,
                field: &str,
            ) -> std::result::Result<Option<T>, types::ToolError> {
                let Some(metadata) = plan.metadata.as_ref() else {
                    return Ok(None);
                };
                let Some(value) = metadata.get(field) else {
                    return Ok(None);
                };

                serde_json::from_value(value.clone())
                    .map(Some)
                    .map_err(|error| {
                        types::ToolError::Tool(format!("Invalid '{field}' metadata: {error}"))
                    })
            }

            pub fn require_mode_input<'a>(
                plan: &'a types::ExecutionPlan,
                field: &'static str,
            ) -> std::result::Result<&'a str, types::ToolError> {
                plan.input
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        types::ToolError::Tool(format!(
                            "Execution plan requires non-empty '{field}'."
                        ))
                    })
            }

            pub fn map_anyhow_error(error: anyhow::Error) -> types::ToolError {
                types::ToolError::Tool(error.to_string())
            }
        }
        pub mod modes {
            pub mod interactive {
                use anyhow::Result;
                use tokio::sync::mpsc;

                use crate::runtime::orchestrator::kernel::{
                    ExecutionKernel, map_anyhow_error, parse_optional_metadata, require_mode_input,
                };
                use crate::runtime::session_runner::{
                    SessionExecutionResult, SessionInputMode, SessionTurnRuntimeOptions,
                };
                use ::agent::StreamDisplayMode;
                use ::agent::agent::StreamEmitter;
                use types::{ChatSession, SteerMessage};
                use types::{ExecutionOutcome, ExecutionPlan};

                #[derive(Debug, Clone)]
                pub struct InteractiveExecutionResult {
                    pub session: ChatSession,
                    pub execution: SessionExecutionResult,
                    pub outcome: ExecutionOutcome,
                }

                pub async fn run_with_session(
                    kernel: &ExecutionKernel,
                    session: &mut ChatSession,
                    user_input: &str,
                    max_history: usize,
                    input_mode: SessionInputMode,
                    emitter: Option<Box<dyn StreamEmitter>>,
                    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                ) -> Result<InteractiveExecutionResult> {
                    run_with_session_options(
                        kernel,
                        session,
                        user_input,
                        max_history,
                        input_mode,
                        emitter,
                        SessionTurnRuntimeOptions {
                            steer_rx,
                            stream_display_mode: StreamDisplayMode::Buffered,
                            workspace_root: None,
                        },
                    )
                    .await
                }

                pub async fn run_with_session_options(
                    kernel: &ExecutionKernel,
                    session: &mut ChatSession,
                    user_input: &str,
                    max_history: usize,
                    input_mode: SessionInputMode,
                    emitter: Option<Box<dyn StreamEmitter>>,
                    options: SessionTurnRuntimeOptions,
                ) -> Result<InteractiveExecutionResult> {
                    let execution = kernel
                        .backend()
                        .execute_interactive_session_turn(
                            session,
                            user_input,
                            max_history,
                            input_mode,
                            emitter,
                            options,
                        )
                        .await?;
                    let outcome = ExecutionOutcome {
                        success: true,
                        text: Some(execution.output.clone()),
                        iterations: Some(execution.iterations),
                        model: Some(execution.final_model.as_serialized_str().to_string()),
                        metadata: Some(serde_json::json!({
                            "chat_session_id": session.id,
                            "resolved_agent_id": session.agent_id,
                        })),
                        ..ExecutionOutcome::default()
                    };

                    Ok(InteractiveExecutionResult {
                        session: session.clone(),
                        execution,
                        outcome,
                    })
                }

                pub async fn run_plan(
                    kernel: &ExecutionKernel,
                    plan: ExecutionPlan,
                ) -> std::result::Result<ExecutionOutcome, types::ToolError> {
                    let session_id = plan.chat_session_id.as_deref().ok_or_else(|| {
                        types::ToolError::Tool(
                            "Interactive execution requires 'chat_session_id'.".to_string(),
                        )
                    })?;
                    let mut session = kernel
                        .backend()
                        .load_chat_session(session_id)
                        .map_err(map_anyhow_error)?;
                    let input = require_mode_input(&plan, "input")?;
                    let max_history = parse_optional_metadata::<usize>(&plan, "max_history")?
                        .unwrap_or(types::DEFAULT_CHAT_MAX_SESSION_HISTORY);
                    let input_mode =
                        parse_optional_metadata::<SessionInputModeWrapper>(&plan, "input_mode")?
                            .map(Into::into)
                            .unwrap_or(SessionInputMode::EphemeralInput);

                    run_with_session(
                        kernel,
                        &mut session,
                        input,
                        max_history,
                        input_mode,
                        None,
                        None,
                    )
                    .await
                    .map(|result| result.outcome)
                    .map_err(map_anyhow_error)
                }

                #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                #[serde(rename_all = "snake_case")]
                enum SessionInputModeWrapper {
                    PersistedInSession,
                    EphemeralInput,
                }

                impl From<SessionInputModeWrapper> for SessionInputMode {
                    fn from(value: SessionInputModeWrapper) -> Self {
                        match value {
                            SessionInputModeWrapper::PersistedInSession => {
                                SessionInputMode::PersistedInSession
                            }
                            SessionInputModeWrapper::EphemeralInput => {
                                SessionInputMode::EphemeralInput
                            }
                        }
                    }
                }
            }
            pub mod subagent {
                use crate::runtime::orchestrator::kernel::{ExecutionKernel, map_anyhow_error};
                use types::{ExecutionOutcome, ExecutionPlan};

                pub async fn run_plan(
                    kernel: &ExecutionKernel,
                    plan: ExecutionPlan,
                ) -> std::result::Result<ExecutionOutcome, types::ToolError> {
                    kernel
                        .backend()
                        .execute_subagent_plan(plan)
                        .await
                        .map_err(map_anyhow_error)
                }
            }
        }
        #[allow(clippy::module_inception)]
        pub mod orchestrator {
            use std::path::PathBuf;
            use std::sync::Arc;
            use std::time::Instant;

            use anyhow::Result;
            use async_trait::async_trait;
            use tokio::sync::mpsc;

            use crate::runtime::orchestrator::kernel::{ExecutionBackend, ExecutionKernel};
            use crate::runtime::orchestrator::modes::{interactive, subagent};
            use crate::runtime::session_runner::{
                AgentRuntimeExecutor, SessionInputMode, SessionTurnRuntimeOptions,
            };
            use ::agent::StreamDisplayMode;
            use ::agent::agent::{NullEmitter, StreamEmitter};
            use types::{AgentOrchestrator, ExecutionOutcome, ExecutionPlan, ToolError};
            use types::{ChatSession, SteerMessage};

            #[derive(Debug)]
            pub struct TracedInteractiveExecutionResult {
                pub turn_id: String,
                pub duration_ms: u64,
                pub execution: crate::runtime::session_runner::SessionExecutionResult,
            }

            pub struct InteractiveSessionRequest<'a> {
                pub session: &'a mut ChatSession,
                pub user_input: &'a str,
                pub max_history: usize,
                pub input_mode: SessionInputMode,
                pub run_id: String,
                pub timeout_secs: Option<u64>,
                pub emitter: Option<Box<dyn StreamEmitter>>,
                pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                pub stream_display_mode: StreamDisplayMode,
                pub workspace_root: Option<PathBuf>,
            }

            #[derive(Debug)]
            pub enum InteractiveExecutionError {
                Timeout { timeout_secs: u64 },
                Execution(anyhow::Error),
            }

            impl std::fmt::Display for InteractiveExecutionError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        Self::Timeout { timeout_secs } => {
                            write!(f, "execution timed out after {} seconds", timeout_secs)
                        }
                        Self::Execution(error) => write!(f, "{error}"),
                    }
                }
            }

            impl std::error::Error for InteractiveExecutionError {}

            #[derive(Clone)]
            pub struct AgentOrchestratorImpl {
                kernel: Arc<ExecutionKernel>,
            }

            impl AgentOrchestratorImpl {
                pub fn new(backend: Arc<dyn ExecutionBackend>) -> Self {
                    Self {
                        kernel: Arc::new(ExecutionKernel::new(backend)),
                    }
                }

                pub fn from_runtime_executor(executor: AgentRuntimeExecutor) -> Self {
                    Self::new(Arc::new(executor))
                }

                pub async fn run_interactive_session_turn(
                    &self,
                    session: &mut ChatSession,
                    user_input: &str,
                    max_history: usize,
                    input_mode: SessionInputMode,
                    emitter: Option<Box<dyn StreamEmitter>>,
                    steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                ) -> Result<interactive::InteractiveExecutionResult> {
                    interactive::run_with_session(
                        self.kernel.as_ref(),
                        session,
                        user_input,
                        max_history,
                        input_mode,
                        emitter,
                        steer_rx,
                    )
                    .await
                }

                async fn run_interactive_session_turn_with_options(
                    &self,
                    session: &mut ChatSession,
                    user_input: &str,
                    max_history: usize,
                    input_mode: SessionInputMode,
                    emitter: Option<Box<dyn StreamEmitter>>,
                    options: SessionTurnRuntimeOptions,
                ) -> Result<interactive::InteractiveExecutionResult> {
                    interactive::run_with_session_options(
                        self.kernel.as_ref(),
                        session,
                        user_input,
                        max_history,
                        input_mode,
                        emitter,
                        options,
                    )
                    .await
                }

                pub async fn run_traced_interactive_session_turn(
                    &self,
                    request: InteractiveSessionRequest<'_>,
                ) -> std::result::Result<TracedInteractiveExecutionResult, InteractiveExecutionError>
                {
                    let InteractiveSessionRequest {
                        session,
                        user_input,
                        max_history,
                        input_mode,
                        run_id,
                        timeout_secs,
                        emitter,
                        steer_rx,
                        stream_display_mode,
                        workspace_root,
                    } = request;
                    self.kernel
                        .backend()
                        .prepare_interactive_session(session)
                        .map_err(InteractiveExecutionError::Execution)?;

                    let inner_emitter = emitter.unwrap_or_else(|| Box::new(NullEmitter));
                    let traced_emitter: Box<dyn StreamEmitter> = inner_emitter;

                    let started_at = Instant::now();
                    let execution_result = if let Some(timeout_secs) = timeout_secs {
                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(timeout_secs),
                            self.run_interactive_session_turn_with_options(
                                session,
                                user_input,
                                max_history,
                                input_mode,
                                Some(traced_emitter),
                                SessionTurnRuntimeOptions {
                                    steer_rx,
                                    stream_display_mode,
                                    workspace_root,
                                },
                            ),
                        )
                        .await
                        {
                            Ok(result) => result.map_err(InteractiveExecutionError::Execution),
                            Err(_) => {
                                let duration_ms = started_at.elapsed().as_millis() as u64;
                                let error = InteractiveExecutionError::Timeout { timeout_secs };
                                let _ = duration_ms;
                                return Err(error);
                            }
                        }
                    } else {
                        self.run_interactive_session_turn_with_options(
                            session,
                            user_input,
                            max_history,
                            input_mode,
                            Some(traced_emitter),
                            SessionTurnRuntimeOptions {
                                steer_rx,
                                stream_display_mode,
                                workspace_root,
                            },
                        )
                        .await
                        .map_err(InteractiveExecutionError::Execution)
                    };

                    let execution = match execution_result {
                        Ok(result) => result.execution,
                        Err(error) => {
                            return Err(error);
                        }
                    };

                    let duration_ms = started_at.elapsed().as_millis() as u64;

                    Ok(TracedInteractiveExecutionResult {
                        turn_id: run_id,
                        duration_ms,
                        execution,
                    })
                }
            }

            #[async_trait]
            impl AgentOrchestrator for AgentOrchestratorImpl {
                async fn run(
                    &self,
                    plan: ExecutionPlan,
                ) -> std::result::Result<ExecutionOutcome, ToolError> {
                    plan.validate()?;
                    match plan.mode.clone().expect("validated mode") {
                        types::ExecutionMode::Interactive => {
                            interactive::run_plan(self.kernel.as_ref(), plan).await
                        }
                        types::ExecutionMode::Subagent => {
                            subagent::run_plan(self.kernel.as_ref(), plan).await
                        }
                    }
                }
            }

            #[cfg(test)]
            mod tests {
                use std::sync::{Arc, Mutex};

                use anyhow::Result;
                use async_trait::async_trait;

                use crate::runtime::orchestrator::kernel::ExecutionBackend;
                use crate::runtime::session_runner::{SessionExecutionResult, SessionInputMode};
                use ::agent::agent::StreamEmitter;
                use types::{ChatSession, ModelId};
                use types::{ExecutionMode, ExecutionPlan, InlineSubagentConfig};

                use super::*;

                #[derive(Default)]
                struct MockBackend {
                    session: Mutex<Option<ChatSession>>,
                }

                #[async_trait]
                impl ExecutionBackend for MockBackend {
                    fn load_chat_session(&self, _session_id: &str) -> Result<ChatSession> {
                        self.session
                            .lock()
                            .expect("session lock")
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("missing session"))
                    }

                    async fn execute_interactive_session_turn(
                        &self,
                        session: &mut ChatSession,
                        _user_input: &str,
                        _max_history: usize,
                        _input_mode: SessionInputMode,
                        _emitter: Option<Box<dyn StreamEmitter>>,
                        _options: SessionTurnRuntimeOptions,
                    ) -> Result<SessionExecutionResult> {
                        session.agent_id = "fallback-agent".to_string();
                        let result = SessionExecutionResult::new(
                            "interactive-output".to_string(),
                            3,
                            "gpt-5.3-codex".to_string(),
                            ModelId::CodexCli,
                        );
                        Ok(result)
                    }

                    async fn execute_subagent_plan(
                        &self,
                        _plan: ExecutionPlan,
                    ) -> Result<ExecutionOutcome> {
                        Ok(ExecutionOutcome {
                            success: true,
                            text: Some("subagent-output".to_string()),
                            ..ExecutionOutcome::default()
                        })
                    }
                }

                #[tokio::test]
                async fn run_interactive_session_turn_updates_session_and_result() {
                    let backend = Arc::new(MockBackend::default());
                    let mut session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());
                    backend
                        .session
                        .lock()
                        .expect("session lock")
                        .replace(session.clone());
                    let orchestrator = AgentOrchestratorImpl::new(backend);

                    let result = orchestrator
                        .run_interactive_session_turn(
                            &mut session,
                            "hello",
                            20,
                            SessionInputMode::EphemeralInput,
                            None,
                            None,
                        )
                        .await
                        .expect("interactive run should succeed");

                    assert_eq!(session.agent_id, "fallback-agent");
                    assert_eq!(result.execution.output, "interactive-output");
                    assert_eq!(result.outcome.iterations, Some(3));
                    assert_eq!(result.outcome.model.as_deref(), Some("gpt-5.3-codex"));
                }
                #[tokio::test]
                async fn run_traced_interactive_session_turn_returns_timeout_error() {
                    #[derive(Default)]
                    struct SlowBackend;

                    #[async_trait]
                    impl ExecutionBackend for SlowBackend {
                        fn load_chat_session(&self, _session_id: &str) -> Result<ChatSession> {
                            Ok(ChatSession::new("agent-a".to_string(), "gpt-5".to_string()))
                        }

                        async fn execute_interactive_session_turn(
                            &self,
                            _session: &mut ChatSession,
                            _user_input: &str,
                            _max_history: usize,
                            _input_mode: SessionInputMode,
                            _emitter: Option<Box<dyn StreamEmitter>>,
                            _options: SessionTurnRuntimeOptions,
                        ) -> Result<SessionExecutionResult> {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            Ok(SessionExecutionResult::new(
                                "too-late".to_string(),
                                1,
                                "gpt-5".to_string(),
                                ModelId::Gpt5,
                            ))
                        }

                        async fn execute_subagent_plan(
                            &self,
                            _plan: ExecutionPlan,
                        ) -> Result<ExecutionOutcome> {
                            unreachable!("subagent path not used")
                        }
                    }

                    let orchestrator = AgentOrchestratorImpl::new(Arc::new(SlowBackend));
                    let mut session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());

                    let error = orchestrator
                        .run_traced_interactive_session_turn(InteractiveSessionRequest {
                            session: &mut session,
                            user_input: "hello",
                            max_history: 20,
                            input_mode: SessionInputMode::EphemeralInput,
                            run_id: "run-timeout".to_string(),
                            timeout_secs: Some(0),
                            emitter: None,
                            steer_rx: None,
                            stream_display_mode: StreamDisplayMode::Buffered,
                            workspace_root: None,
                        })
                        .await
                        .expect_err("interactive run should time out");

                    assert!(matches!(
                        error,
                        InteractiveExecutionError::Timeout { timeout_secs: 0 }
                    ));
                }

                #[tokio::test]
                async fn run_plan_dispatches_interactive_mode() {
                    let backend = Arc::new(MockBackend::default());
                    let session = ChatSession::new("agent-a".to_string(), "gpt-5".to_string());
                    let session_id = session.id.clone();
                    backend
                        .session
                        .lock()
                        .expect("session lock")
                        .replace(session);
                    let orchestrator = AgentOrchestratorImpl::new(backend);

                    let outcome = orchestrator
                        .run(ExecutionPlan {
                            mode: Some(ExecutionMode::Interactive),
                            agent_id: Some("agent-a".to_string()),
                            chat_session_id: Some(session_id),
                            input: Some("hello".to_string()),
                            ..ExecutionPlan::default()
                        })
                        .await
                        .expect("interactive plan should succeed");

                    assert!(outcome.success);
                    assert_eq!(outcome.text.as_deref(), Some("interactive-output"));
                }

                #[tokio::test]
                async fn run_plan_dispatches_subagent_mode() {
                    let orchestrator = AgentOrchestratorImpl::new(Arc::new(MockBackend::default()));

                    let outcome = orchestrator
                        .run(ExecutionPlan {
                            mode: Some(ExecutionMode::Subagent),
                            input: Some("task".to_string()),
                            inline_subagent: Some(InlineSubagentConfig::default()),
                            ..ExecutionPlan::default()
                        })
                        .await
                        .expect("subagent mode should delegate");

                    assert_eq!(outcome.text.as_deref(), Some("subagent-output"));
                }
            }
        }

        pub use kernel::{ExecutionBackend, ExecutionKernel};
        pub use orchestrator::{
            AgentOrchestratorImpl, InteractiveExecutionError, InteractiveSessionRequest,
            TracedInteractiveExecutionResult,
        };
    }
    pub mod session_runner {
        //! Agent session runtime module.
        //!
        //! This module owns the runtime-side session execution path. Delegated
        //! sub-agent execution remains an `agent` capability injected into this runtime
        //! when needed.
        //!
        //! # Architecture
        //!
        //! - `executor`: Real agent executor that bridges to `agent`
        //! - `retry`: Retry mechanism for transient failures
        //! - `failover`: Model failover system for automatic fallback
        //! # Execution
        //!
        //! Interactive TUI turns and sub-agent turns both go through this runtime.
        //!
        //! # Usage
        //!
        //! ```ignore
        //! use runner::runtime::session_runner::{
        //!     AgentRuntimeExecutor, RetryConfig, FailoverConfig, FailoverManager
        //! };
        //!
        //! // For API-based execution:
        //! let executor = Arc::new(AgentRuntimeExecutor::new(
        //!     storage.clone(),
        //!     subagent_tracker.clone(),
        //!     subagent_definitions.clone(),
        //!     subagent_config.clone(),
        //! ));
        //!
        //! let _executor = executor;
        //! ```
        //!
        //! # Retry Example
        //!
        //! ```ignore
        //! use runner::runtime::session_runner::retry::{RetryConfig, RetryState};
        //!
        //! let config = RetryConfig::default();
        //! let mut state = RetryState::new();
        //!
        //! // After a failure
        //! if state.should_retry(&config, "Connection timeout") {
        //!     state.record_failure("Connection timeout", &config);
        //!     // Wait before retrying
        //! }
        //! ```
        //!
        //! # Failover Example
        //!
        //! ```ignore
        //! use runner::runtime::session_runner::failover::{FailoverConfig, FailoverManager};
        //! use crate::ModelId;
        //!
        //! let config = FailoverConfig::with_fallbacks(
        //!     ModelId::ClaudeSonnet4_5,
        //!     vec![ModelId::Gpt5, ModelId::DeepseekChat],
        //! );
        //! let manager = FailoverManager::new(config);
        //!
        //! // Get the best available model
        //! if let Some(model) = manager.get_available_model().await {
        //!     // Use this model
        //! }
        //! ```
        //!
        use ::agent::llm::Message;
        use types::ModelId;

        pub mod error_classification {
            use ::agent::AiError;
            use anyhow::Error as AnyhowError;
            use types::{ToolErrorCategory, ToolOutput};

            use super::{
                ExecutionErrorClassification, ExecutionErrorKind, ExecutionFailure, RetryClass,
            };

            pub fn classify_execution_error(error: &AnyhowError) -> ExecutionErrorClassification {
                if let Some(ai_error) = error.downcast_ref::<AiError>() {
                    return classify_ai_error(ai_error);
                }

                classify_execution_error_message(&error.to_string())
            }

            pub fn classify_execution_error_message(message: &str) -> ExecutionErrorClassification {
                let lower = message.to_lowercase();

                if contains_any(
                    &lower,
                    &[
                        "interrupted",
                        "cancelled",
                        "canceled",
                        "user interrupt",
                        "user requested stop",
                    ],
                ) {
                    return ExecutionErrorClassification::new(
                        ExecutionErrorKind::UserInterrupted,
                        RetryClass::NonRetryable,
                    );
                }

                if contains_any(
                    &lower,
                    &[
                        "unauthorized",
                        "forbidden",
                        "authentication",
                        "auth failed",
                        "invalid api key",
                        "invalid token",
                        "api key",
                        "api_key",
                        "secret",
                        "credential",
                        "401",
                        "403",
                        "billing",
                    ],
                ) {
                    return ExecutionErrorClassification::new(
                        ExecutionErrorKind::Authentication,
                        RetryClass::NonRetryable,
                    );
                }

                if contains_any(
                    &lower,
                    &[
                        "rate limit",
                        "rate-limit",
                        "too many requests",
                        "retry after",
                        "retry-after",
                        "quota",
                        "429",
                    ],
                ) {
                    return ExecutionErrorClassification::new(
                        ExecutionErrorKind::RateLimited,
                        RetryClass::Retryable,
                    );
                }

                if contains_any(
                    &lower,
                    &[
                        "timeout",
                        "timed out",
                        "connection refused",
                        "connection reset",
                        "connection aborted",
                        "broken pipe",
                        "transport error",
                        "connection closed",
                        "network error",
                        "network unreachable",
                        "error sending request",
                        "request failed",
                        "temporary failure",
                        "temporarily unavailable",
                        "service unavailable",
                        "internal server error",
                        "500",
                        "503",
                        "504",
                        "502",
                        "bad gateway",
                        "gateway timeout",
                        "overloaded",
                        "capacity",
                        "please try again",
                    ],
                ) {
                    return ExecutionErrorClassification::new(
                        ExecutionErrorKind::Timeout,
                        RetryClass::Retryable,
                    );
                }

                if contains_any(
                    &lower,
                    &[
                        "bad request",
                        "invalid request",
                        "validation error",
                        "invalid model",
                        "model not found",
                        "configuration error",
                        "not found",
                        "404",
                        "400",
                    ],
                ) {
                    return ExecutionErrorClassification::new(
                        ExecutionErrorKind::Validation,
                        RetryClass::NonRetryable,
                    );
                }

                if contains_any(&lower, &["tool error", "tool not found"]) {
                    return ExecutionErrorClassification::new(
                        ExecutionErrorKind::Tool,
                        RetryClass::NonRetryable,
                    );
                }

                ExecutionErrorClassification::new(
                    ExecutionErrorKind::Internal,
                    RetryClass::NonRetryable,
                )
            }

            pub fn classify_tool_output_failure(output: &ToolOutput) -> ExecutionFailure {
                let classification = match output.error_category {
                    Some(ToolErrorCategory::Auth) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::Authentication,
                        RetryClass::NonRetryable,
                    ),
                    Some(ToolErrorCategory::RateLimit) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::RateLimited,
                        RetryClass::Retryable,
                    ),
                    Some(ToolErrorCategory::Network) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::Timeout,
                        RetryClass::Retryable,
                    ),
                    Some(ToolErrorCategory::Config | ToolErrorCategory::NotFound) => {
                        ExecutionErrorClassification::new(
                            ExecutionErrorKind::Validation,
                            RetryClass::NonRetryable,
                        )
                    }
                    Some(ToolErrorCategory::Execution) | None => {
                        if output.retryable.unwrap_or(false) {
                            ExecutionErrorClassification::new(
                                ExecutionErrorKind::Tool,
                                RetryClass::Retryable,
                            )
                        } else {
                            ExecutionErrorClassification::new(
                                ExecutionErrorKind::Tool,
                                RetryClass::NonRetryable,
                            )
                        }
                    }
                };

                ExecutionFailure {
                    message: output
                        .error
                        .clone()
                        .unwrap_or_else(|| "Tool execution failed".to_string()),
                    classification,
                    cause: None,
                }
            }

            pub fn is_retryable_classification(
                classification: ExecutionErrorClassification,
            ) -> bool {
                matches!(classification.retry_class, RetryClass::Retryable)
            }

            pub fn is_authentication_classification(
                classification: ExecutionErrorClassification,
            ) -> bool {
                matches!(classification.kind, ExecutionErrorKind::Authentication)
            }

            fn classify_ai_error(error: &AiError) -> ExecutionErrorClassification {
                match error {
                    AiError::LlmHttp { status, .. } => match status {
                        401 | 403 => ExecutionErrorClassification::new(
                            ExecutionErrorKind::Authentication,
                            RetryClass::NonRetryable,
                        ),
                        429 => ExecutionErrorClassification::new(
                            ExecutionErrorKind::RateLimited,
                            RetryClass::Retryable,
                        ),
                        502..=504 => ExecutionErrorClassification::new(
                            ExecutionErrorKind::Timeout,
                            RetryClass::Retryable,
                        ),
                        400 | 404 | 422 => ExecutionErrorClassification::new(
                            ExecutionErrorKind::Validation,
                            RetryClass::NonRetryable,
                        ),
                        _ => ExecutionErrorClassification::new(
                            ExecutionErrorKind::Model,
                            RetryClass::NonRetryable,
                        ),
                    },
                    AiError::Http(error) if error.is_timeout() || error.is_connect() => {
                        ExecutionErrorClassification::new(
                            ExecutionErrorKind::Timeout,
                            RetryClass::Retryable,
                        )
                    }
                    AiError::Http(_) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::Internal,
                        RetryClass::NonRetryable,
                    ),
                    AiError::Tool(_) | AiError::ToolNotFound(_) => {
                        ExecutionErrorClassification::new(
                            ExecutionErrorKind::Tool,
                            RetryClass::NonRetryable,
                        )
                    }
                    AiError::InvalidFormat(_) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::Validation,
                        RetryClass::NonRetryable,
                    ),
                    AiError::MaxIterations(_) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::Internal,
                        RetryClass::NonRetryable,
                    ),
                    AiError::Agent(message) | AiError::Llm(message) => {
                        classify_execution_error_message(message)
                    }
                    AiError::Json(_) | AiError::Io(_) => ExecutionErrorClassification::new(
                        ExecutionErrorKind::Internal,
                        RetryClass::NonRetryable,
                    ),
                }
            }

            fn contains_any(haystack: &str, needles: &[&str]) -> bool {
                needles.iter().any(|needle| haystack.contains(needle))
            }

            #[cfg(test)]
            mod tests {
                use super::*;

                #[test]
                fn classifies_authentication_messages() {
                    let classification = classify_execution_error_message("HTTP 401 unauthorized");
                    assert_eq!(classification.kind, ExecutionErrorKind::Authentication);
                    assert_eq!(classification.retry_class, RetryClass::NonRetryable);
                }

                #[test]
                fn classifies_rate_limit_messages() {
                    let classification =
                        classify_execution_error_message("429 rate limit exceeded");
                    assert_eq!(classification.kind, ExecutionErrorKind::RateLimited);
                    assert_eq!(classification.retry_class, RetryClass::Retryable);
                }

                #[test]
                fn classifies_tool_output_failure_from_category() {
                    let failure = classify_tool_output_failure(&ToolOutput::non_retryable_error(
                        "missing config",
                        ToolErrorCategory::Config,
                    ));
                    assert_eq!(failure.classification.kind, ExecutionErrorKind::Validation);
                    assert_eq!(failure.classification.retry_class, RetryClass::NonRetryable);
                }

                #[test]
                fn classifies_timeout_messages() {
                    let classification =
                        classify_execution_error_message("503 Service Unavailable");
                    assert_eq!(classification.kind, ExecutionErrorKind::Timeout);
                    assert_eq!(classification.retry_class, RetryClass::Retryable);
                }

                #[test]
                fn classifies_reqwest_send_failures_as_timeout() {
                    let classification = classify_execution_error_message(
                        "LLM error: Request failed: error sending request for url (https://api.minimax.io/anthropic/v1/messages)",
                    );
                    assert_eq!(classification.kind, ExecutionErrorKind::Timeout);
                    assert_eq!(classification.retry_class, RetryClass::Retryable);
                }

                #[test]
                fn classifies_validation_messages() {
                    let classification = classify_execution_error_message("400 Bad Request");
                    assert_eq!(classification.kind, ExecutionErrorKind::Validation);
                    assert_eq!(classification.retry_class, RetryClass::NonRetryable);
                }

                #[test]
                fn classifies_interrupt_messages() {
                    let classification =
                        classify_execution_error_message("Execution interrupted by user");
                    assert_eq!(classification.kind, ExecutionErrorKind::UserInterrupted);
                    assert_eq!(classification.retry_class, RetryClass::NonRetryable);
                }

                #[test]
                fn classifies_unknown_messages_as_internal() {
                    let classification = classify_execution_error_message("unexpected panic");
                    assert_eq!(classification.kind, ExecutionErrorKind::Internal);
                    assert_eq!(classification.retry_class, RetryClass::NonRetryable);
                }
            }
        }
        pub mod executor {
            //! Agent/session executor implementation.
            //!
            //! This module provides `AgentRuntimeExecutor`, which implements the
            //! It loads agent configuration from storage, builds the appropriate LLM
            //! client, and executes the agent with the configured tools.

            use anyhow::{Result, anyhow};
            use std::collections::{HashMap, HashSet};
            use std::sync::Arc;
            use std::time::Duration;

            use crate::runtime::{AgentOrchestratorImpl, ExecutionContext};
            use crate::tools::{ReplyTool, SwitchModelTool};
            use crate::{
                ModelId, Provider, provider_policy::resolve_model_from_available_secrets,
                services::session::SessionService, storage::Storage,
            };
            use ::agent::agent::{LlmToolCallReviewer, SharedStreamEmitter, StreamEmitter};
            use ::agent::llm::Message;
            use ::agent::{
                AgentConfig as ReActAgentConfig, AgentExecutor as ReActAgentExecutor, CodexClient,
                DefaultLlmClientFactory, LlmClient, LlmClientFactory,
                ResourceLimits as AgentResourceLimits, SwappableLlm,
            };
            use tokio::sync::mpsc;
            use tokio::time::sleep;
            use tracing::{debug, info, warn};
            use types::llm::LlmProvider;
            use types::{
                AgentNode, ApiKeyConfig, ChatMessage, ChatRole, ChatSession, ChatTurnEventKind,
                ChatTurnStatus, Skill, SteerMessage,
            };
            use types::{ExecutionOutcome, ExecutionPlan, ReplySender};

            use super::SessionExecutionResult;
            use super::failover::{FailoverConfig, FailoverManager, execute_with_failover};
            use super::retry::{RetryConfig, RetryState};
            use crate::runtime::agent::{
                BashConfig, SkillActivationPolicy, ToolRegistry, build_agent_system_prompt,
                effective_tool_allowlist_for_turn, main_agent_default_tool_names,
                registry_from_allowlist, secret_resolver_from_storage,
            };
            use ::agent::agent::SubagentDefLookup;
            use ::agent::agent::{
                SubagentConfig, SubagentExecutionBridge, SubagentTracker, execute_subagent_plan,
            };
            use ::agent::llm::LlmSwitcherImpl;
            use preflight::SkillSnapshotCache;
            #[cfg(any(test, feature = "test-utils"))]
            use std::sync::{Mutex, OnceLock};

            fn share_stream_emitter(
                emitter: Option<Box<dyn StreamEmitter>>,
            ) -> Option<SharedStreamEmitter> {
                emitter.map(SharedStreamEmitter::new)
            }

            fn clone_shared_emitter(
                emitter: &Option<SharedStreamEmitter>,
            ) -> Option<Box<dyn StreamEmitter>> {
                emitter
                    .as_ref()
                    .map(|shared| Box::new(shared.clone()) as Box<dyn StreamEmitter>)
            }

            #[cfg(any(test, feature = "test-utils"))]
            type TestLlmFactorySlot = Mutex<Option<Arc<dyn LlmClientFactory>>>;

            #[cfg(any(test, feature = "test-utils"))]
            fn test_llm_factory_slot() -> &'static TestLlmFactorySlot {
                static SLOT: OnceLock<TestLlmFactorySlot> = OnceLock::new();
                SLOT.get_or_init(|| Mutex::new(None))
            }

            #[cfg(any(test, feature = "test-utils"))]
            pub struct TestLlmFactoryGuard {
                previous: Option<Arc<dyn LlmClientFactory>>,
            }

            #[cfg(any(test, feature = "test-utils"))]
            impl Drop for TestLlmFactoryGuard {
                fn drop(&mut self) {
                    *test_llm_factory_slot()
                        .lock()
                        .expect("test llm factory slot lock") = self.previous.take();
                }
            }

            #[cfg(any(test, feature = "test-utils"))]
            pub fn install_test_llm_factory(
                factory: Arc<dyn LlmClientFactory>,
            ) -> TestLlmFactoryGuard {
                let mut guard = test_llm_factory_slot()
                    .lock()
                    .expect("test llm factory slot lock");
                let previous = guard.replace(factory);
                TestLlmFactoryGuard { previous }
            }

            #[cfg(any(test, feature = "test-utils"))]
            fn current_test_llm_factory() -> Option<Arc<dyn LlmClientFactory>> {
                test_llm_factory_slot()
                    .lock()
                    .expect("test llm factory slot lock")
                    .clone()
            }

            /// Real agent executor that bridges to ::agent::AgentExecutor.
            ///
            /// This executor:
            /// - Loads agent configuration from storage
            /// - Resolves API keys (direct or from secrets)
            /// - Creates the appropriate LLM client for the model
            /// - Builds the system prompt from the agent's skill
            /// - Executes the agent via the ReAct loop
            #[derive(Clone)]
            pub struct AgentRuntimeExecutor {
                storage: Arc<Storage>,
                subagent_tracker: Arc<SubagentTracker>,
                subagent_definitions: Arc<dyn SubagentDefLookup>,
                subagent_config: SubagentConfig,
                session_service: SessionService,
                skill_snapshot_cache: Arc<SkillSnapshotCache>,
                reply_sender: Option<Arc<dyn ReplySender>>,
            }

            const TOOL_RESULT_CONTEXT_RATIO: f64 = 0.08;
            const TOOL_RESULT_MIN_CHARS: usize = 512;
            const TOOL_RESULT_MAX_CHARS: usize = 24_000;
            const TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE: usize = 4;
            /// Controls whether the latest user input has already been persisted
            /// to the chat session before execution.
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum SessionInputMode {
                /// Latest user input is already stored as the newest session message.
                PersistedInSession,
                /// Latest user input is provided only as runtime input for this turn.
                EphemeralInput,
            }

            #[derive(Debug, Clone)]
            struct ResolvedSkillSnapshot {
                resolved_skills: Vec<Skill>,
            }

            impl AgentRuntimeExecutor {
                pub(crate) fn load_chat_session(&self, session_id: &str) -> Result<ChatSession> {
                    self.session_service
                        .get_session_view(session_id)?
                        .ok_or_else(|| anyhow!("Session not found: {}", session_id))
                }

                pub(crate) async fn execute_subagent_plan(
                    &self,
                    plan: ExecutionPlan,
                ) -> Result<ExecutionOutcome> {
                    let llm_client: Arc<dyn LlmClient> = Arc::new(CodexClient::new());
                    let factory: Arc<dyn LlmClientFactory> =
                        Arc::new(DefaultLlmClientFactory::new(
                            self.build_api_keys(None, Provider::OpenAI).await,
                            ModelId::build_model_specs(),
                        ));
                    let swappable = Arc::new(SwappableLlm::new(llm_client.clone()));
                    let agent_defaults = self
                        .storage
                        .config
                        .get_effective_config()
                        .ok()
                        .map(|config| config.agent)
                        .unwrap_or_default();
                    let bash_config = BashConfig {
                        timeout_secs: agent_defaults.bash_timeout_secs,
                        ..BashConfig::default()
                    };
                    let default_tools = main_agent_default_tool_names();
                    let tool_registry = self.build_tool_registry(
                        Some(&default_tools),
                        llm_client.clone(),
                        swappable,
                        factory.clone(),
                        None,
                        Some(bash_config),
                        None,
                        None,
                    )?;
                    execute_subagent_plan(
                        self.subagent_definitions.clone(),
                        llm_client,
                        tool_registry,
                        self.subagent_config.clone(),
                        plan,
                        SubagentExecutionBridge {
                            llm_client_factory: Some(factory),
                            orchestrator: None,
                        },
                    )
                    .await
                    .map_err(|error| anyhow!(error.to_string()))
                }

                /// Create a new AgentRuntimeExecutor with access to storage.
                pub fn new(
                    storage: Arc<Storage>,
                    subagent_tracker: Arc<SubagentTracker>,
                    subagent_definitions: Arc<dyn SubagentDefLookup>,
                    subagent_config: SubagentConfig,
                ) -> Self {
                    let session_service = SessionService::from_storage(storage.as_ref());
                    Self {
                        storage,
                        subagent_tracker,
                        subagent_definitions,
                        subagent_config,
                        session_service,
                        skill_snapshot_cache: Arc::new(SkillSnapshotCache::default()),
                        reply_sender: None,
                    }
                }

                #[cfg(test)]
                pub(crate) fn with_session_service(
                    mut self,
                    session_service: SessionService,
                ) -> Self {
                    self.session_service = session_service;
                    self
                }

                /// Set a reply sender so the agent can send intermediate messages.
                pub fn with_reply_sender(mut self, sender: Arc<dyn ReplySender>) -> Self {
                    self.reply_sender = Some(sender);
                    self
                }
            }

            mod model_resolution {
                use super::*;
                use anyhow::Context;
                use serde::Deserialize;
                use std::path::PathBuf;
                use std::time::Instant;
                use tokio::sync::{OnceCell, RwLock};
                use types::provider_meta;

                impl AgentRuntimeExecutor {
                    pub(super) fn build_llm_factory(
                        api_keys: HashMap<LlmProvider, String>,
                        model_specs: Vec<types::ModelSpec>,
                    ) -> Arc<dyn LlmClientFactory> {
                        #[cfg(any(test, feature = "test-utils"))]
                        if let Some(factory) = current_test_llm_factory() {
                            return factory;
                        }

                        Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs))
                    }

                    pub(super) fn should_skip_api_key_resolution() -> bool {
                        #[cfg(any(test, feature = "test-utils"))]
                        {
                            return current_test_llm_factory().is_some();
                        }

                        #[allow(unreachable_code)]
                        false
                    }

                    pub(super) async fn resolve_api_key(
                        &self,
                        provider: Provider,
                        agent_api_key_config: Option<&ApiKeyConfig>,
                    ) -> Result<String> {
                        // First, check agent-level API key config
                        if let Some(config) = agent_api_key_config {
                            match config {
                                ApiKeyConfig::Direct(key) => {
                                    if !key.is_empty() {
                                        return Ok(key.clone());
                                    }
                                }
                                ApiKeyConfig::Secret(secret_name) => {
                                    if let Some(secret_value) =
                                        self.storage.secrets.get_secret(secret_name)?
                                    {
                                        return Ok(secret_value);
                                    }
                                    return Err(anyhow!("Secret '{}' not found", secret_name));
                                }
                            }
                        }

                        // Use well-known secret names for each provider.
                        let Some(secret_name) = provider.api_key_env() else {
                            return Err(anyhow!(
                                "No API key fallback is defined for provider {:?}.",
                                provider
                            ));
                        };

                        for secret_name in provider.api_key_env_candidates() {
                            if let Some(secret_value) =
                                self.storage.secrets.get_secret(secret_name)?
                            {
                                return Ok(secret_value);
                            }
                        }

                        Err(anyhow!(
                            "No API key configured for provider {:?}. Please add secret '{}' in Settings.",
                            provider,
                            secret_name
                        ))
                    }

                    /// Resolve API key, avoiding mismatched agent-level keys for fallback providers.
                    pub(super) async fn resolve_api_key_for_model(
                        &self,
                        provider: Provider,
                        agent_api_key_config: Option<&ApiKeyConfig>,
                        primary_provider: Provider,
                    ) -> Result<String> {
                        let config = if provider == primary_provider {
                            agent_api_key_config
                        } else {
                            None
                        };
                        self.resolve_api_key(provider, config).await
                    }

                    pub(super) fn context_window_for_model(model: ModelId) -> usize {
                        match model {
                            ModelId::ClaudeOpus4_6
                            | ModelId::ClaudeSonnet4_5
                            | ModelId::ClaudeHaiku4_5
                            | ModelId::ClaudeCodeOpus
                            | ModelId::ClaudeCodeSonnet
                            | ModelId::ClaudeCodeHaiku => 200_000,
                            ModelId::Gpt5
                            | ModelId::Gpt5Mini
                            | ModelId::Gpt5Nano
                            | ModelId::Gpt5Pro
                            | ModelId::Gpt5_1
                            | ModelId::Gpt5_2
                            | ModelId::Gpt5Codex
                            | ModelId::Gpt5_1Codex
                            | ModelId::Gpt5_2Codex
                            | ModelId::CodexCli => 128_000,
                            ModelId::Gpt5_4 | ModelId::Gpt5_4Codex => 1_000_000,
                            ModelId::Gpt5_4Mini
                            | ModelId::Gpt5_4Nano
                            | ModelId::Gpt5_4MiniCodex => 400_000,
                            ModelId::DeepseekChat | ModelId::DeepseekReasoner => 64_000,
                            ModelId::Gemini25Pro
                            | ModelId::Gemini25Flash
                            | ModelId::Gemini3Pro
                            | ModelId::Gemini3Flash
                            | ModelId::GeminiCli => 1_000_000,
                            _ => 128_000,
                        }
                    }

                    pub(super) async fn resolve_model_from_stored_credentials(
                        &self,
                    ) -> Result<Option<ModelId>> {
                        Ok(resolve_model_from_available_secrets(|key| {
                            self.storage
                                .secrets
                                .has_available_secret(key)
                                .unwrap_or(false)
                        }))
                    }

                    pub(super) async fn resolve_primary_model(
                        &self,
                        agent_node: &AgentNode,
                    ) -> Result<ModelId> {
                        if let Some(model_ref) = agent_node.resolved_model_ref() {
                            return Ok(model_ref.model);
                        }

                        if let Some(model) = self.resolve_model_from_stored_credentials().await? {
                            info!(
                                selected_model = %model.as_str(),
                                "Resolved model from stored credentials for agent without explicit model"
                            );
                            return Ok(model);
                        }

                        Err(anyhow!(
                            "Model not specified. Please set a model for this agent or configure a compatible API secret."
                        ))
                    }

                    pub(super) async fn build_api_keys(
                        &self,
                        agent_api_key_config: Option<&ApiKeyConfig>,
                        primary_provider: Provider,
                    ) -> HashMap<LlmProvider, String> {
                        let mut keys = HashMap::new();

                        for provider in Provider::all().iter().copied() {
                            if provider.api_key_env().is_none() {
                                continue;
                            }
                            if let Ok(key) = self
                                .resolve_api_key_for_model(
                                    provider,
                                    agent_api_key_config,
                                    primary_provider,
                                )
                                .await
                            {
                                keys.insert(provider.as_llm_provider(), key);
                            }
                        }

                        keys
                    }

                    pub(super) fn create_llm_client(
                        factory: &dyn LlmClientFactory,
                        model: ModelId,
                        api_key: Option<&str>,
                        agent_node: &AgentNode,
                    ) -> Result<Arc<dyn LlmClient>> {
                        if model.is_codex_cli() {
                            let mut client = CodexClient::new().with_model(model.as_str());
                            if let Some(effort) = agent_node
                                .codex_cli_reasoning_effort
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                            {
                                client = client.with_reasoning_effort(effort);
                            }
                            if let Some(mode) = agent_node.codex_cli_execution_mode.as_ref() {
                                client = client.with_execution_mode(mode.as_str());
                            }
                            return Ok(Arc::new(client));
                        }

                        Ok(factory.create_client(model.as_serialized_str(), api_key)?)
                    }
                }

                const DEFAULT_MODELS_BASE_URL: &str = "https://models.dev";
                const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);
                const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
                const OUTPUT_TOKENS_CAP: usize = 32_000;

                #[derive(Debug, Clone, Copy)]
                pub(super) struct ModelCapabilities {
                    pub context_window: usize,
                    pub input_limit: Option<usize>,
                    pub output_limit: usize,
                }

                #[derive(Debug, Clone, Copy)]
                pub(super) struct ModelCatalogEntry {
                    pub capabilities: ModelCapabilities,
                }

                #[derive(Debug, Default)]
                struct CatalogState {
                    by_provider_model: HashMap<String, ModelCatalogEntry>,
                    by_model: HashMap<String, ModelCatalogEntry>,
                    last_refresh: Option<Instant>,
                }

                pub(super) struct ModelCatalog {
                    client: reqwest::Client,
                    cache_path: PathBuf,
                    state: RwLock<CatalogState>,
                }

                static GLOBAL_MODEL_CATALOG: OnceCell<Arc<ModelCatalog>> = OnceCell::const_new();

                impl ModelCatalog {
                    pub async fn global() -> Arc<Self> {
                        GLOBAL_MODEL_CATALOG
                            .get_or_init(|| async {
                                let client = reqwest::Client::builder()
                                    .timeout(REQUEST_TIMEOUT)
                                    .build()
                                    .unwrap_or_else(|_| reqwest::Client::new());
                                let cache_path = resolve_cache_path().unwrap_or_else(|_| {
                                    PathBuf::from(".restflow").join("cache").join("models.json")
                                });

                                let catalog = Arc::new(Self {
                                    client,
                                    cache_path,
                                    state: RwLock::new(CatalogState::default()),
                                });
                                catalog.load_cache_if_present().await;
                                catalog.refresh_if_stale(true).await;
                                catalog
                            })
                            .await
                            .clone()
                    }

                    pub async fn resolve(&self, model: ModelId) -> Option<ModelCatalogEntry> {
                        self.refresh_if_stale(false).await;

                        let state = self.state.read().await;
                        let provider_ids = models_dev_provider_candidates(model.provider());
                        let model_ids = model_id_candidates(model);

                        for provider_id in provider_ids {
                            for model_id in &model_ids {
                                let key = provider_model_key(provider_id, model_id);
                                if let Some(entry) = state.by_provider_model.get(&key) {
                                    return Some(*entry);
                                }
                            }
                        }

                        for model_id in &model_ids {
                            let key = normalize(model_id);
                            if let Some(entry) = state.by_model.get(&key) {
                                return Some(*entry);
                            }
                        }

                        None
                    }

                    async fn load_cache_if_present(&self) {
                        let raw = match std::fs::read_to_string(&self.cache_path) {
                            Ok(raw) => raw,
                            Err(_) => return,
                        };

                        if let Ok(parsed) = parse_models_dev_json(&raw) {
                            let mut state = self.state.write().await;
                            state.by_provider_model = parsed.by_provider_model;
                            state.by_model = parsed.by_model;
                            state.last_refresh = Some(Instant::now());
                        }
                    }

                    async fn refresh_if_stale(&self, force: bool) {
                        if models_fetch_disabled() {
                            return;
                        }

                        {
                            let state = self.state.read().await;
                            if !force
                                && state
                                    .last_refresh
                                    .is_some_and(|last| last.elapsed() < DEFAULT_REFRESH_INTERVAL)
                            {
                                return;
                            }
                        }

                        let mut state = self.state.write().await;
                        if !force
                            && state
                                .last_refresh
                                .is_some_and(|last| last.elapsed() < DEFAULT_REFRESH_INTERVAL)
                        {
                            return;
                        }
                        state.last_refresh = Some(Instant::now());
                        drop(state);

                        let url = models_url();
                        match self.client.get(&url).send().await {
                            Ok(response) if response.status().is_success() => {
                                match response.text().await {
                                    Ok(raw) => match parse_models_dev_json(&raw) {
                                        Ok(parsed) => {
                                            {
                                                let mut state = self.state.write().await;
                                                state.by_provider_model = parsed.by_provider_model;
                                                state.by_model = parsed.by_model;
                                            }
                                            if let Err(err) = write_cache(&self.cache_path, &raw) {
                                                warn!(error = %err, "Failed to persist models.dev cache");
                                            }
                                            debug!("Refreshed models.dev catalog");
                                        }
                                        Err(err) => {
                                            warn!(error = %err, "Failed to parse models.dev payload");
                                            self.state.write().await.last_refresh = None;
                                        }
                                    },
                                    Err(err) => {
                                        warn!(error = %err, "Failed to read models.dev response body");
                                        self.state.write().await.last_refresh = None;
                                    }
                                }
                            }
                            Ok(response) => {
                                warn!(
                                    status = response.status().as_u16(),
                                    "models.dev returned non-success status"
                                );
                                self.state.write().await.last_refresh = None;
                            }
                            Err(err) => {
                                debug!(error = %err, "Skipping models.dev refresh due to request error");
                                self.state.write().await.last_refresh = None;
                            }
                        }
                    }
                }

                #[derive(Debug, Default)]
                struct ParsedCatalog {
                    by_provider_model: HashMap<String, ModelCatalogEntry>,
                    by_model: HashMap<String, ModelCatalogEntry>,
                }

                #[derive(Debug, Deserialize)]
                struct ModelsDevProvider {
                    models: HashMap<String, ModelsDevModel>,
                }

                #[derive(Debug, Deserialize)]
                struct ModelsDevModel {
                    id: Option<String>,
                    limit: ModelsDevLimit,
                }

                #[derive(Debug, Deserialize)]
                struct ModelsDevLimit {
                    context: u64,
                    input: Option<u64>,
                    output: u64,
                }

                fn parse_models_dev_json(raw: &str) -> Result<ParsedCatalog> {
                    let root: HashMap<String, ModelsDevProvider> = serde_json::from_str(raw)
                        .context("Failed to deserialize models.dev JSON")?;

                    let mut parsed = ParsedCatalog::default();
                    for (provider_id, provider) in root {
                        for (model_key, model) in provider.models {
                            let context_window = model.limit.context as usize;
                            if context_window == 0 {
                                continue;
                            }

                            let output_limit = if model.limit.output == 0 {
                                OUTPUT_TOKENS_CAP
                            } else {
                                (model.limit.output as usize).min(OUTPUT_TOKENS_CAP)
                            };

                            let entry = ModelCatalogEntry {
                                capabilities: ModelCapabilities {
                                    context_window,
                                    input_limit: model.limit.input.map(|v| v as usize),
                                    output_limit,
                                },
                            };

                            insert_entry(&mut parsed, &provider_id, &model_key, entry);
                            if let Some(model_id) = model.id.as_deref() {
                                insert_entry(&mut parsed, &provider_id, model_id, entry);
                            }
                        }
                    }

                    Ok(parsed)
                }

                fn insert_entry(
                    parsed: &mut ParsedCatalog,
                    provider_id: &str,
                    model_id: &str,
                    entry: ModelCatalogEntry,
                ) {
                    parsed
                        .by_provider_model
                        .insert(provider_model_key(provider_id, model_id), entry);
                    parsed.by_model.entry(normalize(model_id)).or_insert(entry);
                }

                fn provider_model_key(provider_id: &str, model_id: &str) -> String {
                    format!("{}::{}", normalize(provider_id), normalize(model_id))
                }

                fn normalize(value: &str) -> String {
                    value.trim().to_ascii_lowercase()
                }

                fn model_id_candidates(model: ModelId) -> Vec<String> {
                    let mut candidates = Vec::new();
                    let mut seen = HashSet::new();
                    let mut push = |value: String| {
                        let trimmed = value.trim();
                        if trimmed.is_empty() {
                            return;
                        }
                        let key = normalize(trimmed);
                        if seen.insert(key) {
                            candidates.push(trimmed.to_string());
                        }
                    };

                    let raw_ids = [model.as_str(), model.as_serialized_str()];
                    for id in raw_ids {
                        push(id.to_string());
                        push(id.replace('.', "-"));
                        push(id.replace('-', "."));

                        if let Some(base) = id.strip_suffix("-preview") {
                            push(base.to_string());
                        }
                        if let Some((_, tail)) = id.split_once('/') {
                            push(tail.to_string());
                            push(tail.replace('.', "-"));
                            push(tail.replace('-', "."));
                        }
                    }

                    candidates
                }

                fn models_dev_provider_candidates(provider: Provider) -> &'static [&'static str] {
                    provider_meta(provider.as_model_provider()).models_dev_provider_ids
                }

                fn models_url() -> String {
                    let configured = std::env::var("RESTFLOW_MODELS_URL")
                        .ok()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());

                    match configured {
                        Some(url) if url.ends_with(".json") => url,
                        Some(url) => format!("{}/api.json", url.trim_end_matches('/')),
                        None => format!("{}/api.json", DEFAULT_MODELS_BASE_URL),
                    }
                }

                fn models_fetch_disabled() -> bool {
                    std::env::var("RESTFLOW_DISABLE_MODELS_FETCH")
                        .ok()
                        .map(|raw| {
                            matches!(
                                raw.trim().to_ascii_lowercase().as_str(),
                                "1" | "true" | "yes" | "on"
                            )
                        })
                        .unwrap_or(false)
                }

                fn resolve_cache_path() -> Result<PathBuf> {
                    if let Ok(path) = std::env::var("RESTFLOW_MODELS_PATH")
                        && !path.trim().is_empty()
                    {
                        return Ok(PathBuf::from(path));
                    }

                    let cache_dir = crate::paths::ensure_restflow_dir()?.join("cache");
                    std::fs::create_dir_all(&cache_dir)?;
                    Ok(cache_dir.join("models.json"))
                }

                fn write_cache(path: &PathBuf, raw: &str) -> Result<()> {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, raw)?;
                    Ok(())
                }
            }
            mod preflight {
                use super::*;
                use regex::Regex;
                use sha2::{Digest, Sha256};
                use std::sync::RwLock;
                use types::SkillPreflightPolicyMode;

                #[derive(Debug, Clone, PartialEq, Eq)]
                pub struct PreflightResult {
                    pub passed: bool,
                    pub blockers: Vec<PreflightIssue>,
                    pub warnings: Vec<PreflightIssue>,
                }

                #[derive(Debug, Clone, PartialEq, Eq)]
                pub struct PreflightIssue {
                    pub category: PreflightCategory,
                    pub message: String,
                    pub suggestion: Option<String>,
                }

                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub enum PreflightCategory {
                    MissingTool,
                    MissingSecret,
                    UnsetVariable,
                    InvalidConfig,
                }

                impl PreflightCategory {
                    pub fn as_str(self) -> &'static str {
                        match self {
                            Self::MissingTool => "missing_tool",
                            Self::MissingSecret => "missing_secret",
                            Self::UnsetVariable => "unset_variable",
                            Self::InvalidConfig => "invalid_config",
                        }
                    }
                }

                pub fn run_preflight(
                    skills: &[Skill],
                    available_tools: &[String],
                    skill_variables: Option<&HashMap<String, String>>,
                    model_configured: bool,
                    policy_mode: SkillPreflightPolicyMode,
                ) -> PreflightResult {
                    let mut blockers = Vec::new();
                    let mut warnings = Vec::new();
                    let mut skill_warnings = Vec::new();

                    if !model_configured {
                        blockers.push(PreflightIssue {
                            category: PreflightCategory::InvalidConfig,
                            message: "No model configured for agent".to_string(),
                            suggestion: Some(
                                "Set model in agent definition or configure provider credentials"
                                    .into(),
                            ),
                        });
                    }

                    if policy_mode != SkillPreflightPolicyMode::Off {
                        let available_tool_set: HashSet<&str> =
                            available_tools.iter().map(String::as_str).collect();
                        for skill in skills {
                            for tool_name in &skill.suggested_tools {
                                if !available_tool_set.contains(tool_name.as_str()) {
                                    skill_warnings.push(PreflightIssue {
                                        category: PreflightCategory::MissingTool,
                                        message: format!(
                                            "Suggested tool '{}' from skill '{}' is not available",
                                            tool_name, skill.id
                                        ),
                                        suggestion: Some(
                                            "Check tool allowlist or remove from suggested_tools"
                                                .into(),
                                        ),
                                    });
                                }
                            }
                        }

                        let variable_map = skill_variables.cloned().unwrap_or_default();
                        let variable_regex = Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}")
                            .expect("variable placeholder regex must compile");

                        let mut seen_variables: HashSet<String> = HashSet::new();
                        for skill in skills {
                            for captures in variable_regex.captures_iter(&skill.content) {
                                let variable_name = captures[1].to_string();
                                if !seen_variables.insert(variable_name.clone()) {
                                    continue;
                                }

                                let missing = variable_map
                                    .get(&variable_name)
                                    .map(|value| value.trim().is_empty())
                                    .unwrap_or(true);
                                if missing {
                                    skill_warnings.push(PreflightIssue {
                                        category: PreflightCategory::UnsetVariable,
                                        message: format!(
                                            "Variable '{{{{{}}}}}' is used in skill content but has no value",
                                            variable_name
                                        ),
                                        suggestion: Some("Set value in agent.skill_variables".into()),
                                    });
                                }
                            }
                        }
                    }

                    match policy_mode {
                        SkillPreflightPolicyMode::Off => {}
                        SkillPreflightPolicyMode::Warn => warnings.extend(skill_warnings),
                        SkillPreflightPolicyMode::Enforce => {
                            for issue in skill_warnings {
                                if is_critical_skill_warning(issue.category) {
                                    blockers.push(issue);
                                } else {
                                    warnings.push(issue);
                                }
                            }
                        }
                    }

                    PreflightResult {
                        passed: blockers.is_empty(),
                        blockers,
                        warnings,
                    }
                }

                fn is_critical_skill_warning(category: PreflightCategory) -> bool {
                    matches!(
                        category,
                        PreflightCategory::MissingTool | PreflightCategory::UnsetVariable
                    )
                }

                #[derive(Debug, Clone, PartialEq, Eq, Hash)]
                pub(super) struct SkillSnapshotKey {
                    pub agent_id: Option<String>,
                    pub skill_filter_signature: String,
                }

                impl SkillSnapshotKey {
                    pub fn new(agent_id: Option<String>, skill_filter_signature: String) -> Self {
                        Self {
                            agent_id,
                            skill_filter_signature,
                        }
                    }
                }

                #[derive(Debug, Clone, Default)]
                pub(super) struct SkillSnapshotPayload {
                    pub resolved_skills: Vec<Skill>,
                }

                #[derive(Debug, Clone)]
                struct CachedSkillSnapshot {
                    version_hash: String,
                    payload: SkillSnapshotPayload,
                }

                #[derive(Debug, Clone)]
                pub(super) struct SkillSnapshotLookup {
                    pub payload: SkillSnapshotPayload,
                    pub hit: bool,
                }

                #[derive(Debug, Default)]
                pub(super) struct SkillSnapshotCache {
                    entries: RwLock<HashMap<SkillSnapshotKey, CachedSkillSnapshot>>,
                }

                impl SkillSnapshotCache {
                    pub fn resolve_with<F>(
                        &self,
                        key: SkillSnapshotKey,
                        version_hash: String,
                        refresh: F,
                    ) -> Result<SkillSnapshotLookup>
                    where
                        F: FnOnce() -> Result<SkillSnapshotPayload>,
                    {
                        {
                            let entries = self.entries.read().map_err(|error| {
                                anyhow!("Skill snapshot cache lock poisoned: {error}")
                            })?;
                            if let Some(cached) = entries.get(&key)
                                && cached.version_hash == version_hash
                            {
                                return Ok(SkillSnapshotLookup {
                                    payload: cached.payload.clone(),
                                    hit: true,
                                });
                            }
                        }

                        let refreshed = refresh()?;
                        let cached = CachedSkillSnapshot {
                            version_hash,
                            payload: refreshed.clone(),
                        };

                        let mut entries = self.entries.write().map_err(|error| {
                            anyhow!("Skill snapshot cache lock poisoned: {error}")
                        })?;
                        entries.insert(key, cached);

                        Ok(SkillSnapshotLookup {
                            payload: refreshed,
                            hit: false,
                        })
                    }
                }

                pub(super) fn build_skill_filter_signature(
                    skill_filter: Option<&[String]>,
                ) -> String {
                    let mut ids: Vec<&str> = skill_filter
                        .unwrap_or_default()
                        .iter()
                        .map(String::as_str)
                        .collect();
                    ids.sort_unstable();
                    hash_text(&ids.join("|"))
                }

                pub(super) fn build_skill_version_hash(skills: &[Skill]) -> String {
                    let mut versions: Vec<String> = skills
                        .iter()
                        .map(|skill| {
                            let fallback_content_hash =
                                hex::encode(Sha256::digest(skill.content.as_bytes()));
                            let content_version_hash = skill
                                .content_hash
                                .as_ref()
                                .cloned()
                                .unwrap_or(fallback_content_hash);
                            format!("{}:{}:{}", skill.id, skill.updated_at, content_version_hash)
                        })
                        .collect();
                    versions.sort_unstable();
                    hash_text(&versions.join("\n"))
                }

                fn hash_text(input: &str) -> String {
                    hex::encode(Sha256::digest(input.as_bytes()))
                }

                impl AgentRuntimeExecutor {
                    pub(super) fn resolve_effective_tool_names(
                        &self,
                        agent_node: &AgentNode,
                        _agent_id: Option<&str>,
                        user_input: Option<&str>,
                    ) -> Result<Vec<String>> {
                        let skills = crate::services::skills::list_available_skills()?;
                        let result = effective_tool_allowlist_for_turn(
                            agent_node,
                            user_input,
                            &skills,
                            SkillActivationPolicy::IgnoreInvalid,
                        )?;
                        Ok(result.tool_names)
                    }

                    pub(super) fn resolve_preflight_available_tool_names(
                        &self,
                        agent_node: &AgentNode,
                        user_input: Option<&str>,
                    ) -> Result<Vec<String>> {
                        let requested_tools =
                            self.resolve_effective_tool_names(agent_node, None, user_input)?;
                        let registry = registry_from_allowlist(
                            Some(&requested_tools),
                            None,
                            Some(secret_resolver_from_storage(&self.storage)),
                            Some(self.storage.as_ref()),
                            None,
                            None,
                            None,
                        )?;
                        let mut available_tools = registry
                            .list()
                            .into_iter()
                            .map(str::to_string)
                            .collect::<Vec<_>>();

                        for caller_registered in [
                            "spawn_subagent",
                            "spawn_subagent_batch",
                            "wait_subagents",
                            "list_subagents",
                            "switch_model",
                            "reply",
                        ] {
                            if requested_tools.iter().any(|tool| tool == caller_registered)
                                && !available_tools.iter().any(|tool| tool == caller_registered)
                            {
                                available_tools.push(caller_registered.to_string());
                            }
                        }

                        Ok(available_tools)
                    }

                    pub(super) fn resolve_preflight_skills(
                        &self,
                        agent_node: &AgentNode,
                        user_input: Option<&str>,
                    ) -> Result<Vec<Skill>> {
                        self.resolve_skill_snapshot(agent_node, None, user_input)
                            .map(|snapshot| snapshot.resolved_skills)
                    }

                    pub(super) fn resolve_skill_snapshot(
                        &self,
                        agent_node: &AgentNode,
                        agent_id: Option<&str>,
                        _user_input: Option<&str>,
                    ) -> Result<ResolvedSkillSnapshot> {
                        let key = SkillSnapshotKey::new(
                            agent_id.map(|value| value.to_string()),
                            build_skill_filter_signature(agent_node.skills.as_deref()),
                        );

                        let all_skills = crate::services::skills::list_available_skills()?;
                        let version_hash = build_skill_version_hash(&all_skills);

                        let assigned_skill_ids = agent_node.skills.clone().unwrap_or_default();

                        let lookup = self
                            .skill_snapshot_cache
                            .resolve_with(key, version_hash, move || {
                                let skill_by_id: HashMap<String, Skill> = all_skills
                                    .into_iter()
                                    .map(|skill| (skill.id.clone(), skill))
                                    .collect();
                                let mut resolved_skills = Vec::new();
                                for skill_id in assigned_skill_ids {
                                    match skill_by_id.get(&skill_id) {
                                        Some(skill) => resolved_skills.push(skill.clone()),
                                        None => {
                                            warn!(skill_id = %skill_id, "Skill referenced by agent not found during preflight")
                                        }
                                    }
                                }

                                Ok(SkillSnapshotPayload {
                                    resolved_skills,
                                })
                            })?;

                        if lookup.hit {
                            debug!("Skill snapshot cache hit");
                        } else {
                            debug!("Skill snapshot cache miss");
                        }

                        Ok(ResolvedSkillSnapshot {
                            resolved_skills: lookup.payload.resolved_skills,
                        })
                    }

                    pub(super) async fn run_preflight_check(
                        &self,
                        agent_node: &AgentNode,
                        primary_model: ModelId,
                        primary_provider: Provider,
                        user_input: Option<&str>,
                    ) -> Result<()> {
                        let skills = self.resolve_preflight_skills(agent_node, user_input)?;
                        let available_tools =
                            self.resolve_preflight_available_tool_names(agent_node, user_input)?;
                        let mut preflight = run_preflight(
                            &skills,
                            &available_tools,
                            agent_node.skill_variables.as_ref(),
                            true,
                            agent_node.effective_skill_preflight_policy_mode(),
                        );

                        if !Self::should_skip_api_key_resolution()
                            && !primary_model.is_codex_cli()
                            && !primary_model.is_gemini_cli()
                            && let Err(error) = self
                                .resolve_api_key_for_model(
                                    primary_provider,
                                    agent_node.api_key_config.as_ref(),
                                    primary_provider,
                                )
                                .await
                        {
                            preflight.blockers.push(PreflightIssue {
                                category: PreflightCategory::MissingSecret,
                                message: error.to_string(),
                                suggestion: Some("Configure API key via secrets".to_string()),
                            });
                            preflight.passed = false;
                        }

                        for warning_issue in &preflight.warnings {
                            warn!(
                                category = warning_issue.category.as_str(),
                                message = %warning_issue.message,
                                suggestion = ?warning_issue.suggestion,
                                "Agent preflight warning"
                            );
                        }

                        if !preflight.passed {
                            let blocker_message = preflight
                                .blockers
                                .iter()
                                .map(|issue| {
                                    format!("- [{}] {}", issue.category.as_str(), issue.message)
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            return Err(anyhow!("Preflight check failed:\n{}", blocker_message));
                        }

                        Ok(())
                    }
                }
            }
            mod session_execution {
                use super::*;
                use crate::services::adapters::SkrunSkillProvider;
                use crate::services::skill_mentions::parse_skill_mentions;
                use ::agent::StreamDisplayMode;
                use types::skill::{SkillInfo, SkillProvider};

                fn should_force_non_stream(model: ModelId) -> bool {
                    model.is_cli_model()
                }

                fn interactive_turn_failover_config(primary: ModelId) -> FailoverConfig {
                    // Interactive turns may execute side-effecting tools. Retrying the whole
                    // ReAct turn on a fallback model can replay already-run tool calls.
                    FailoverConfig::with_fallbacks(primary, Vec::new())
                }

                #[derive(Default)]
                pub struct SessionTurnRuntimeOptions {
                    pub steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                    pub stream_display_mode: StreamDisplayMode,
                    pub workspace_root: Option<std::path::PathBuf>,
                }

                impl AgentRuntimeExecutor {
                    pub(crate) fn resolve_stored_agent_for_session(
                        &self,
                        session: &mut ChatSession,
                    ) -> Result<crate::StoredAgent> {
                        if let Some(agent) =
                            self.storage.agents.get_agent(session.agent_id.clone())?
                        {
                            return Ok(agent);
                        }

                        let fallback = self.storage.agents.resolve_default_agent()?;

                        let fallback_model = fallback
                            .agent
                            .resolved_model_ref()
                            .map(|model_ref| model_ref.model.as_serialized_str().to_string())
                            .unwrap_or_else(|| ModelId::Gpt5_4.as_serialized_str().to_string());
                        session.agent_id = fallback.id.clone();
                        session.set_model_identity_from_raw(&fallback_model);

                        Ok(fallback)
                    }

                    fn chat_message_to_llm_message(message: &ChatMessage) -> Message {
                        match message.role {
                            ChatRole::User => Message::user(message.content.clone()),
                            ChatRole::Assistant => Message::assistant(message.content.clone()),
                            ChatRole::System => Message::system(message.content.clone()),
                        }
                    }

                    fn resolve_mentioned_skill_infos(&self, user_input: &str) -> Vec<SkillInfo> {
                        let mentioned_ids = parse_skill_mentions(user_input);
                        if mentioned_ids.is_empty() {
                            return Vec::new();
                        }

                        let provider = SkrunSkillProvider::default();
                        let skills = provider.list_skills();
                        mentioned_ids
                            .into_iter()
                            .filter_map(|id| skills.iter().find(|skill| skill.id == id).cloned())
                            .collect()
                    }

                    fn append_mentioned_skill_directive(
                        mut system_prompt: String,
                        mentioned_skills: &[SkillInfo],
                    ) -> String {
                        if mentioned_skills.is_empty() {
                            return system_prompt;
                        }

                        system_prompt.push_str("\n\n## User-Mentioned Skills\n");
                        system_prompt.push_str(
                            "The latest user message explicitly mentioned these skills. Before applying a mentioned skill, call `load_skill` with `action=read` and `id` set to the skill id.\n\n",
                        );
                        for skill in mentioned_skills {
                            let description =
                                skill.description.as_deref().unwrap_or("No description");
                            system_prompt.push_str(&format!(
                                "- {} ({}): {}\n",
                                skill.name, skill.id, description
                            ));
                        }
                        system_prompt
                    }

                    fn session_messages_for_context(session: &ChatSession) -> Vec<ChatMessage> {
                        if session.turns.iter().any(|turn| !turn.events.is_empty()) {
                            return Self::completed_turn_messages_for_context(session);
                        }

                        if session.messages.is_empty() {
                            return Vec::new();
                        }

                        if let Some(summary_id) = session.summary_message_id.as_ref()
                            && let Some(idx) =
                                session.messages.iter().position(|m| &m.id == summary_id)
                        {
                            let mut messages = session.messages[idx..].to_vec();
                            if let Some(summary) = messages.first_mut() {
                                summary.role = ChatRole::User;
                            }
                            return messages;
                        }

                        session.messages.clone()
                    }

                    fn completed_turn_messages_for_context(
                        session: &ChatSession,
                    ) -> Vec<ChatMessage> {
                        let mut messages = Vec::new();

                        for turn in &session.turns {
                            if turn.status != ChatTurnStatus::Completed {
                                continue;
                            }

                            let mut user_message: Option<String> = None;
                            let mut assistant_message: Option<String> = None;
                            for event in &turn.events {
                                match &event.kind {
                                    ChatTurnEventKind::UserMessage { content }
                                        if user_message.is_none() && !content.trim().is_empty() =>
                                    {
                                        user_message = Some(content.clone());
                                    }
                                    ChatTurnEventKind::AssistantMessage { content }
                                        if !content.trim().is_empty() =>
                                    {
                                        assistant_message = Some(content.clone());
                                    }
                                    _ => {}
                                }
                            }

                            if let (Some(user), Some(assistant)) = (user_message, assistant_message)
                            {
                                messages.push(ChatMessage::user(user));
                                messages.push(ChatMessage::assistant(assistant));
                            }
                        }

                        messages
                    }

                    fn session_history_messages(
                        session: &ChatSession,
                        max_messages: usize,
                        input_mode: SessionInputMode,
                    ) -> Vec<Message> {
                        let mut messages = Self::session_messages_for_context(session);
                        if messages.is_empty() {
                            return Vec::new();
                        }

                        // Exclude the latest user input because it will be passed to execute()
                        // separately for persisted-input flows.
                        if input_mode == SessionInputMode::PersistedInSession
                            && matches!(messages.last().map(|m| &m.role), Some(ChatRole::User))
                        {
                            messages.pop();
                        }

                        let start = messages.len().saturating_sub(max_messages);
                        messages[start..]
                            .iter()
                            .map(Self::chat_message_to_llm_message)
                            .collect()
                    }

                    fn session_state_for_execution(
                        system_prompt: String,
                        session: &ChatSession,
                        max_messages: usize,
                        input_mode: SessionInputMode,
                        user_input: &str,
                        max_iterations: usize,
                    ) -> ::agent::AgentState {
                        let mut state = ::agent::AgentState::new(
                            uuid::Uuid::new_v4().to_string(),
                            max_iterations,
                        );
                        state.add_message(Message::system(system_prompt));
                        for message in
                            Self::session_history_messages(session, max_messages, input_mode)
                        {
                            state.add_message(message);
                        }
                        state.add_message(Message::user(user_input.to_string()));
                        state
                    }

                    #[allow(clippy::too_many_arguments)]
                    async fn execute_session_with_client(
                        &self,
                        agent_node: &AgentNode,
                        model: ModelId,
                        llm_client: Arc<dyn LlmClient>,
                        session: &ChatSession,
                        user_input: &str,
                        max_history: usize,
                        input_mode: SessionInputMode,
                        emitter: Option<Box<dyn StreamEmitter>>,
                        factory: Arc<dyn LlmClientFactory>,
                        agent_id: Option<&str>,
                        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                        stream_display_mode: StreamDisplayMode,
                        workspace_root: Option<std::path::PathBuf>,
                    ) -> Result<SessionExecutionResult> {
                        let swappable = Arc::new(SwappableLlm::new(llm_client));
                        let mentioned_skills = self.resolve_mentioned_skill_infos(user_input);
                        let effective_tools = self.resolve_effective_tool_names(
                            agent_node,
                            agent_id,
                            Some(user_input),
                        )?;
                        let agent_defaults = self
                            .storage
                            .config
                            .get_effective_config_for_workspace(None)
                            .ok()
                            .map(|c| c.agent)
                            .unwrap_or_default();
                        let bash_config = BashConfig {
                            timeout_secs: agent_defaults.bash_timeout_secs,
                            ..BashConfig::default()
                        };
                        let reply_sender = self.resolve_reply_sender(None, agent_id);
                        let tools = self.build_tool_registry(
                            Some(&effective_tools),
                            swappable.clone(),
                            swappable.clone(),
                            factory.clone(),
                            agent_id,
                            Some(bash_config),
                            reply_sender,
                            workspace_root.as_deref(),
                        )?;
                        let system_prompt = Self::append_mentioned_skill_directive(
                            build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?,
                            &mentioned_skills,
                        );

                        let catalog = model_resolution::ModelCatalog::global().await;
                        let model_entry = catalog.resolve(model).await;
                        let context_window = model_entry
                            .map(|entry| {
                                entry
                                    .capabilities
                                    .input_limit
                                    .unwrap_or(entry.capabilities.context_window)
                            })
                            .unwrap_or_else(|| Self::context_window_for_model(model));
                        let max_tool_result_length = Self::effective_max_tool_result_length(
                            agent_defaults.max_tool_result_length,
                            context_window,
                        );
                        let execution_context = ExecutionContext::main(
                            agent_id.unwrap_or(&session.agent_id),
                            &session.id,
                        );

                        let mut config = ReActAgentConfig::new(user_input.to_string())
                            .with_system_prompt(system_prompt.clone())
                            .with_tool_timeout(Duration::from_secs(
                                agent_defaults.tool_timeout_secs,
                            ))
                            .with_max_iterations(agent_defaults.max_iterations)
                            .with_context_window(context_window)
                            .with_resource_limits(Self::chat_resource_limits(
                                agent_defaults.max_tool_calls,
                                agent_defaults.max_wall_clock_secs,
                            ))
                            .with_max_tool_result_length(max_tool_result_length)
                            .with_max_tool_concurrency(agent_defaults.max_tool_concurrency)
                            .with_prune_tool_max_chars(agent_defaults.prune_tool_max_chars)
                            .with_compact_preserve_tokens(agent_defaults.compact_preserve_tokens)
                            .with_stream_display_mode(stream_display_mode);
                        if let Some(entry) = model_entry
                            && !model.is_cli_model()
                        {
                            config = config
                                .with_max_output_tokens(entry.capabilities.output_limit as u32);
                        }
                        if model.supports_temperature()
                            && let Some(temp) = agent_node.temperature
                        {
                            config = config.with_temperature(temp as f32);
                        }
                        config = Self::apply_llm_timeout(config, agent_defaults.llm_timeout_secs);
                        if agent_defaults.auto_review_tools {
                            config = config.with_tool_call_reviewer(Arc::new(
                                LlmToolCallReviewer::new(swappable.clone()),
                            ));
                        }
                        config = Self::apply_execution_context(config, &execution_context);

                        let mut agent = ReActAgentExecutor::new(swappable.clone(), tools)
                            .with_subagent_tracker(self.subagent_tracker.clone());
                        if let Some(workspace_root) = workspace_root.as_ref() {
                            agent = agent.with_workspace_root(workspace_root.clone());
                        }
                        if let Some(rx) = steer_rx {
                            agent = agent.with_steer_channel(rx);
                        }
                        let state = Self::session_state_for_execution(
                            system_prompt,
                            session,
                            max_history,
                            input_mode,
                            user_input,
                            agent_defaults.max_iterations,
                        );
                        let force_non_stream = should_force_non_stream(model);
                        let result = if force_non_stream {
                            if let Some(mut emitter) = emitter {
                                agent
                                    .run_from_state_with_emitter(config, state, emitter.as_mut())
                                    .await?
                            } else {
                                agent.run_from_state(config, state).await?
                            }
                        } else if let Some(mut emitter) = emitter {
                            agent
                                .execute_from_state(config, state, emitter.as_mut())
                                .await?
                        } else {
                            agent.run_from_state(config, state).await?
                        };
                        if !result.success {
                            return Err(anyhow!(
                                "Agent execution failed: {}",
                                result.error.unwrap_or_else(|| "unknown error".to_string())
                            ));
                        }

                        let active_model = swappable.current_model();
                        let final_model =
                            ModelId::for_provider_and_model(model.provider(), &active_model)
                                .or_else(|| ModelId::from_api_name(&active_model))
                                .or_else(|| ModelId::from_canonical_id(&active_model))
                                .unwrap_or(model);
                        let mut execution = SessionExecutionResult::new(
                            result.answer.unwrap_or_default(),
                            result.iterations as u32,
                            active_model,
                            final_model,
                        );
                        execution.metrics.message_count = result.state.messages.len();
                        Ok(execution)
                    }

                    #[allow(clippy::too_many_arguments)]
                    async fn execute_session_with_model(
                        &self,
                        agent_node: &AgentNode,
                        model: ModelId,
                        session: &ChatSession,
                        user_input: &str,
                        primary_provider: Provider,
                        max_history: usize,
                        input_mode: SessionInputMode,
                        emitter: Option<Box<dyn StreamEmitter>>,
                        agent_id: Option<&str>,
                        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
                        stream_display_mode: StreamDisplayMode,
                        workspace_root: Option<std::path::PathBuf>,
                    ) -> Result<SessionExecutionResult> {
                        let model_specs = ModelId::build_model_specs();
                        let api_keys = self
                            .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
                            .await;
                        let factory = Self::build_llm_factory(api_keys, model_specs);

                        let api_key =
                            if Self::should_skip_api_key_resolution() || model.is_codex_cli() {
                                None
                            } else if model.is_gemini_cli() {
                                self.resolve_api_key_for_model(
                                    model.provider(),
                                    agent_node.api_key_config.as_ref(),
                                    primary_provider,
                                )
                                .await
                                .ok()
                            } else {
                                Some(
                                    self.resolve_api_key_for_model(
                                        model.provider(),
                                        agent_node.api_key_config.as_ref(),
                                        primary_provider,
                                    )
                                    .await?,
                                )
                            };

                        let llm_client = Self::create_llm_client(
                            factory.as_ref(),
                            model,
                            api_key.as_deref(),
                            agent_node,
                        )?;
                        self.execute_session_with_client(
                            agent_node,
                            model,
                            llm_client,
                            session,
                            user_input,
                            max_history,
                            input_mode,
                            emitter,
                            factory,
                            agent_id,
                            steer_rx,
                            stream_display_mode,
                            workspace_root,
                        )
                        .await
                    }

                    /// Execute a chat turn for an existing chat session.
                    ///
                    /// This method keeps chat execution in daemon-side runtime logic so UI
                    /// clients (HTTP/MCP/CLI) can share the same execution behavior.
                    pub async fn execute_session_turn(
                        &self,
                        session: &mut ChatSession,
                        user_input: &str,
                        max_history: usize,
                        input_mode: SessionInputMode,
                    ) -> Result<SessionExecutionResult> {
                        self.execute_session_turn_with_emitter(
                            session,
                            user_input,
                            max_history,
                            input_mode,
                            None,
                        )
                        .await
                    }

                    /// Execute a chat turn for an existing chat session with optional stream emitter.
                    pub async fn execute_session_turn_with_emitter(
                        &self,
                        session: &mut ChatSession,
                        user_input: &str,
                        max_history: usize,
                        input_mode: SessionInputMode,
                        emitter: Option<Box<dyn StreamEmitter>>,
                    ) -> Result<SessionExecutionResult> {
                        self.execute_session_turn_with_emitter_and_steer(
                            session,
                            user_input,
                            max_history,
                            input_mode,
                            emitter,
                            SessionTurnRuntimeOptions {
                                steer_rx: None,
                                stream_display_mode: StreamDisplayMode::Buffered,
                                workspace_root: None,
                            },
                        )
                        .await
                    }

                    /// Execute a chat turn for an existing chat session with optional stream emitter
                    /// and optional steer channel.
                    pub async fn execute_session_turn_with_emitter_and_steer(
                        &self,
                        session: &mut ChatSession,
                        user_input: &str,
                        max_history: usize,
                        input_mode: SessionInputMode,
                        emitter: Option<Box<dyn StreamEmitter>>,
                        options: SessionTurnRuntimeOptions,
                    ) -> Result<SessionExecutionResult> {
                        let SessionTurnRuntimeOptions {
                            steer_rx,
                            stream_display_mode,
                            workspace_root,
                        } = options;
                        let stored_agent = self.resolve_stored_agent_for_session(session)?;
                        let agent_node = stored_agent.agent.clone();
                        // Prefer the session's model (user override) over the agent's default
                        let primary_model = if !session.model.is_empty() {
                            match ModelId::from_api_name(&session.model)
                                .or_else(|| ModelId::from_canonical_id(&session.model))
                            {
                                Some(model) => model,
                                None => self.resolve_primary_model(&agent_node).await?,
                            }
                        } else {
                            self.resolve_primary_model(&agent_node).await?
                        };
                        let primary_provider = primary_model.provider();
                        self.run_preflight_check(
                            &agent_node,
                            primary_model,
                            primary_provider,
                            Some(user_input),
                        )
                        .await?;
                        let failover_config = interactive_turn_failover_config(primary_model);
                        let failover_manager = FailoverManager::new(failover_config);
                        let retry_config = RetryConfig::default();
                        let mut retry_state = RetryState::new();
                        let session_snapshot = session.clone();
                        let agent_id = session.agent_id.clone();
                        let shared_emitter = share_stream_emitter(emitter);
                        let mut steer_rx = steer_rx;

                        loop {
                            let node = agent_node.clone();
                            let session_for_execution = session_snapshot.clone();
                            let result = execute_with_failover(&failover_manager, |model| {
                                let node = node.clone();
                                let session_for_execution = session_for_execution.clone();
                                let agent_id = agent_id.clone();
                                let emitter = clone_shared_emitter(&shared_emitter);
                                let steer_rx = steer_rx.take();
                                let workspace_root = workspace_root.clone();
                                async move {
                                    self.execute_session_with_model(
                                        &node,
                                        model,
                                        &session_for_execution,
                                        user_input,
                                        primary_provider,
                                        max_history,
                                        input_mode,
                                        emitter,
                                        Some(agent_id.as_str()),
                                        steer_rx,
                                        stream_display_mode,
                                        workspace_root.clone(),
                                    )
                                    .await
                                }
                            })
                            .await;

                            match result {
                                Ok((mut exec_result, final_model)) => {
                                    exec_result.final_model = final_model;
                                    exec_result.metrics.final_model = Some(final_model);
                                    return Ok(exec_result);
                                }
                                Err(err) => {
                                    let error_msg = err.to_string();
                                    if retry_state.should_retry(&retry_config, &error_msg) {
                                        retry_state.record_failure(&error_msg, &retry_config);
                                        let delay = retry_state.calculate_delay(&retry_config);
                                        sleep(delay).await;
                                        continue;
                                    }
                                    return Err(err);
                                }
                            }
                        }
                    }
                }

                #[cfg(test)]
                mod tests {
                    use super::*;
                    use ::agent::StreamDisplayMode;
                    use ::agent::llm::Role;
                    use types::skill::{SkillInfo, SkillSource};

                    #[test]
                    fn should_force_non_stream_for_all_cli_models() {
                        assert!(should_force_non_stream(ModelId::CodexCli));
                        assert!(should_force_non_stream(ModelId::GeminiCli));
                        assert!(should_force_non_stream(ModelId::OpenCodeCli));
                        assert!(!should_force_non_stream(ModelId::Glm5_1CodingPlan));
                        assert!(!should_force_non_stream(ModelId::Gpt5));
                    }

                    #[test]
                    fn interactive_turn_failover_config_does_not_replay_turn_on_fallbacks() {
                        let config = interactive_turn_failover_config(ModelId::DeepseekChat);

                        assert_eq!(config.primary, ModelId::DeepseekChat);
                        assert!(config.fallbacks.is_empty());
                    }

                    #[test]
                    fn session_turn_runtime_options_default_to_buffered_display() {
                        let options = SessionTurnRuntimeOptions::default();
                        assert_eq!(options.stream_display_mode, StreamDisplayMode::Buffered);
                    }

                    #[test]
                    fn session_history_messages_skip_canceled_turns() {
                        let mut session =
                            ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                        session.add_message(ChatMessage::user("run stale tool"));
                        session.record_turn_user_message("turn-1", "run stale tool");
                        session.record_turn_event(
                            "turn-1",
                            ChatTurnEventKind::ToolCall {
                                call_id: "call-1".to_string(),
                                name: "bash".to_string(),
                                arguments: "{\"command\":\"sleep 15\"}".to_string(),
                            },
                        );
                        session.cancel_turn("turn-1");
                        session.add_message(ChatMessage::user("latest request"));
                        session.record_turn_user_message("turn-2", "latest request");

                        let history = AgentRuntimeExecutor::session_history_messages(
                            &session,
                            20,
                            SessionInputMode::PersistedInSession,
                        );

                        assert!(history.is_empty());
                    }

                    #[test]
                    fn session_state_for_execution_uses_latest_user_after_canceled_tool_turn() {
                        let mut session =
                            ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                        session.add_message(ChatMessage::user("run stale tool"));
                        session.record_turn_user_message("turn-1", "run stale tool");
                        session.record_turn_event(
                            "turn-1",
                            ChatTurnEventKind::ToolCall {
                                call_id: "call-1".to_string(),
                                name: "bash".to_string(),
                                arguments: "{\"command\":\"sleep 20; echo stale\"}".to_string(),
                            },
                        );
                        session.cancel_turn("turn-1");
                        session.add_message(ChatMessage::user("latest request only"));
                        session.record_turn_user_message("turn-2", "latest request only");

                        let state = AgentRuntimeExecutor::session_state_for_execution(
                            "system prompt".to_string(),
                            &session,
                            20,
                            SessionInputMode::PersistedInSession,
                            "latest request only",
                            4,
                        );

                        assert_eq!(state.messages.len(), 2);
                        assert_eq!(state.messages[0].role, Role::System);
                        assert_eq!(state.messages[0].content, "system prompt");
                        assert_eq!(state.messages[1].role, Role::User);
                        assert_eq!(state.messages[1].content, "latest request only");
                        assert!(
                            state
                                .messages
                                .iter()
                                .all(|message| !message.content.contains("stale"))
                        );
                    }

                    #[test]
                    fn session_history_messages_keep_completed_turns() {
                        let mut session =
                            ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                        session.add_message(ChatMessage::user("old request"));
                        session.add_message(ChatMessage::assistant("old answer"));
                        session.record_turn_user_message("turn-1", "old request");
                        session.complete_turn_with_assistant_message("turn-1", "old answer");
                        session.add_message(ChatMessage::user("latest request"));
                        session.record_turn_user_message("turn-2", "latest request");

                        let history = AgentRuntimeExecutor::session_history_messages(
                            &session,
                            20,
                            SessionInputMode::PersistedInSession,
                        );

                        assert_eq!(history.len(), 2);
                        assert_eq!(history[0].role, Role::User);
                        assert_eq!(history[0].content, "old request");
                        assert_eq!(history[1].role, Role::Assistant);
                        assert_eq!(history[1].content, "old answer");
                    }

                    #[test]
                    fn mentioned_skill_directive_lists_ids_without_content() {
                        let prompt = AgentRuntimeExecutor::append_mentioned_skill_directive(
                            "Base prompt".to_string(),
                            &[SkillInfo {
                                id: "team".to_string(),
                                name: "Team".to_string(),
                                description: Some("Coordinate subagents".to_string()),
                                tags: None,
                                kind: Some("markdown".to_string()),
                                executable: false,
                                suggested_tools: Vec::new(),
                                source: SkillSource::System,
                                read_only: true,
                                source_ref: None,
                            }],
                        );

                        assert!(prompt.contains("User-Mentioned Skills"));
                        assert!(prompt.contains("load_skill"));
                        assert!(prompt.contains("Team (team): Coordinate subagents"));
                        assert!(!prompt.contains("# Team"));
                    }
                }
            }
            mod tooling {
                use super::*;
                use ::agent::agent::SubagentManagerImpl;
                use types::SubagentManager;

                impl AgentRuntimeExecutor {
                    pub(super) fn chat_resource_limits(
                        max_tool_calls: usize,
                        max_wall_clock_secs: Option<u64>,
                    ) -> AgentResourceLimits {
                        AgentResourceLimits {
                            max_tool_calls,
                            max_wall_clock: max_wall_clock_secs
                                .map(Duration::from_secs)
                                .unwrap_or(Duration::ZERO),
                            max_depth: AgentResourceLimits::default().max_depth,
                            max_cost_usd: None,
                        }
                    }

                    pub(super) fn apply_llm_timeout(
                        mut config: ReActAgentConfig,
                        llm_timeout_secs: Option<u64>,
                    ) -> ReActAgentConfig {
                        if let Some(timeout_secs) = llm_timeout_secs {
                            config = config.with_llm_timeout(Duration::from_secs(timeout_secs));
                        } else {
                            config = config.without_llm_timeout();
                        }
                        config
                    }

                    pub(super) fn apply_execution_context(
                        mut config: ReActAgentConfig,
                        context: &ExecutionContext,
                    ) -> ReActAgentConfig {
                        config = config.with_context("execution_context", context.to_value());
                        config = config.with_context(
                            "execution_role",
                            serde_json::Value::String(context.role.as_str().to_string()),
                        );
                        if let Some(session_id) = &context.chat_session_id {
                            config = config.with_context(
                                "chat_session_id",
                                serde_json::Value::String(session_id.clone()),
                            );
                        }
                        if let Some(parent_run_id) = &context.parent_run_id {
                            config = config.with_context(
                                "parent_run_id",
                                serde_json::Value::String(parent_run_id.clone()),
                            );
                        }
                        config
                    }

                    pub(super) fn effective_max_tool_result_length(
                        requested_max_output_bytes: usize,
                        context_window: usize,
                    ) -> usize {
                        let requested = requested_max_output_bytes.max(1);
                        let context_token_budget =
                            ((context_window as f64) * TOOL_RESULT_CONTEXT_RATIO).round() as usize;
                        let context_char_budget = context_token_budget
                            .saturating_mul(TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE);
                        let context_cap =
                            context_char_budget.clamp(TOOL_RESULT_MIN_CHARS, TOOL_RESULT_MAX_CHARS);
                        requested.min(context_cap)
                    }

                    pub(super) fn build_subagent_manager(
                        &self,
                        llm_client: Arc<dyn LlmClient>,
                        tool_registry: Arc<ToolRegistry>,
                        llm_client_factory: Arc<dyn LlmClientFactory>,
                    ) -> SubagentManagerImpl {
                        SubagentManagerImpl::new(
                            self.subagent_tracker.clone(),
                            self.subagent_definitions.clone(),
                            llm_client,
                            tool_registry,
                            self.subagent_config.clone(),
                        )
                        .with_llm_client_factory(llm_client_factory)
                        .with_orchestrator(Arc::new(
                            AgentOrchestratorImpl::from_runtime_executor(self.clone()),
                        ))
                    }

                    #[allow(clippy::too_many_arguments)]
                    pub(super) fn build_tool_registry(
                        &self,
                        tool_names: Option<&[String]>,
                        llm_client: Arc<dyn LlmClient>,
                        swappable: Arc<SwappableLlm>,
                        factory: Arc<dyn LlmClientFactory>,
                        agent_id: Option<&str>,
                        bash_config: Option<BashConfig>,
                        reply_sender: Option<Arc<dyn ReplySender>>,
                        workspace_root: Option<&std::path::Path>,
                    ) -> anyhow::Result<Arc<ToolRegistry>> {
                        let has_reply_sender = reply_sender.is_some();
                        let filtered_tool_names =
                            self.filter_requested_tool_names(tool_names, has_reply_sender);
                        let filtered_tool_names_ref = filtered_tool_names.as_deref();
                        let secret_resolver = Some(secret_resolver_from_storage(&self.storage));
                        let subagent_tool_registry = Arc::new(registry_from_allowlist(
                            filtered_tool_names_ref,
                            None,
                            secret_resolver.clone(),
                            Some(self.storage.as_ref()),
                            agent_id,
                            bash_config.clone(),
                            workspace_root,
                        )?);
                        let subagent_manager: Arc<dyn SubagentManager> =
                            Arc::new(self.build_subagent_manager(
                                llm_client,
                                subagent_tool_registry,
                                factory.clone(),
                            ));
                        let mut registry = registry_from_allowlist(
                            filtered_tool_names_ref,
                            Some(subagent_manager),
                            secret_resolver,
                            Some(self.storage.as_ref()),
                            agent_id,
                            bash_config,
                            workspace_root,
                        )?;

                        let requested = |name: &str| {
                            filtered_tool_names_ref
                                .map(|names| names.iter().any(|n| n == name))
                                .unwrap_or(false)
                        };

                        if requested("switch_model") {
                            let switcher = Arc::new(LlmSwitcherImpl::new(swappable, factory));
                            registry.register(SwitchModelTool::new(switcher));
                        }

                        if requested("reply")
                            && let Some(sender) = reply_sender
                        {
                            registry.register(ReplyTool::new(sender));
                        }

                        Ok(Arc::new(registry))
                    }

                    pub(super) fn filter_requested_tool_names(
                        &self,
                        tool_names: Option<&[String]>,
                        has_reply_sender: bool,
                    ) -> Option<Vec<String>> {
                        let names = tool_names?;

                        Some(
                            names
                                .iter()
                                .filter_map(|name| {
                                    if name == "reply" && !has_reply_sender {
                                        debug!(
                                            tool_name = "reply",
                                            "Reply sender missing in this execution context; skipping tool"
                                        );
                                        return None;
                                    }
                                    Some(name.clone())
                                })
                                .collect(),
                        )
                    }

                    pub(super) fn resolve_reply_sender(
                        &self,
                        _task_id: Option<&str>,
                        _agent_id: Option<&str>,
                    ) -> Option<Arc<dyn ReplySender>> {
                        self.reply_sender.clone()
                    }
                }
            }

            pub use session_execution::SessionTurnRuntimeOptions;

            #[cfg(test)]
            mod tests {
                use super::*;
                use crate::provider_policy::provider_default_model;
                use crate::runtime::subagent::AgentDefinitionRegistry;
                use crate::services::session::SessionService;
                use crate::session_log::{FileSession, FileSessionStore};
                use crate::test_support::RestflowTestEnv;
                use ::agent::agent::{SubagentConfig, SubagentTracker};
                use std::future::Future;
                #[cfg(unix)]
                use std::path::PathBuf;
                use std::pin::Pin;
                use tokio::sync::mpsc;
                use types::store::ReplySender;
                use types::{AgentNode, SkillPreflightPolicyMode, SkillSource};

                fn create_test_storage() -> (Arc<Storage>, RestflowTestEnv) {
                    let env = RestflowTestEnv::new();
                    let db_path = env.db_path("test.db");
                    let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                    (Arc::new(storage), env)
                }

                fn create_test_executor(storage: Arc<Storage>) -> AgentRuntimeExecutor {
                    let (completion_tx, completion_rx) = mpsc::channel(10);
                    let subagent_tracker =
                        Arc::new(SubagentTracker::new(completion_tx, completion_rx));
                    let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
                    let subagent_config = SubagentConfig::default();
                    AgentRuntimeExecutor::new(
                        storage,
                        subagent_tracker,
                        subagent_definitions,
                        subagent_config,
                    )
                }

                #[cfg(unix)]
                struct EnvVarGuard {
                    key: &'static str,
                    original: Option<std::ffi::OsString>,
                }

                #[cfg(unix)]
                impl EnvVarGuard {
                    fn set_path(key: &'static str, path: &std::path::Path) -> Self {
                        let original = std::env::var_os(key);
                        unsafe {
                            std::env::set_var(key, path);
                        }
                        Self { key, original }
                    }
                }

                #[cfg(unix)]
                impl Drop for EnvVarGuard {
                    fn drop(&mut self) {
                        if let Some(value) = &self.original {
                            unsafe {
                                std::env::set_var(self.key, value);
                            }
                        } else {
                            unsafe {
                                std::env::remove_var(self.key);
                            }
                        }
                    }
                }

                #[cfg(unix)]
                #[derive(serde::Deserialize)]
                struct TestSkrunSkill {
                    id: String,
                    name: String,
                    version: String,
                    kind: String,
                    #[serde(default)]
                    content: Option<String>,
                    #[serde(default)]
                    suggested_tools: Vec<String>,
                    #[serde(default)]
                    source_ref: Option<String>,
                }

                #[cfg(unix)]
                fn install_skrun_skills(env: &RestflowTestEnv, response: &str) -> PathBuf {
                    let root = env.root().join("skrun-skills");
                    std::fs::create_dir_all(&root).unwrap();
                    let records: Vec<TestSkrunSkill> = serde_json::from_str(response).unwrap();
                    for record in records {
                        let content = record
                            .content
                            .unwrap_or_else(|| format!("# {}", record.name));
                        let mut artifact = match record.kind.as_str() {
                            "markdown" => skrun::SkillArtifact::markdown(
                                record.id,
                                record.name,
                                record.version,
                                content,
                            ),
                            "rust_binary" => {
                                let mut artifact = skrun::SkillArtifact::rust_binary(
                                    record.id,
                                    record.name,
                                    record.version,
                                );
                                artifact.content = Some(content);
                                artifact
                            }
                            other => panic!("unsupported test skrun skill kind: {other}"),
                        };
                        artifact.suggested_tools = record.suggested_tools;
                        artifact.source_ref = record.source_ref;
                        skrun::save_artifact(root.join(&artifact.id), &artifact).unwrap();
                    }
                    root
                }

                #[test]
                fn test_executor_creation() {
                    let (storage, _temp_dir) = create_test_storage();
                    let executor = create_test_executor(storage);
                    // Executor should be created successfully
                    assert!(Arc::strong_count(&executor.storage) >= 1);
                }

                #[test]
                fn load_chat_session_reads_file_session_without_materializing_to_redb() {
                    let (storage, env) = create_test_storage();
                    let file_store = FileSessionStore::new(env.root().join("sessions")).unwrap();
                    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                    session.add_message(ChatMessage::user("run from jsonl"));
                    file_store
                        .write_session(&FileSession::from_chat_session(&session), false)
                        .unwrap();
                    let session_service =
                        SessionService::new(file_store, Some(storage.agents.clone()));
                    let executor =
                        create_test_executor(storage.clone()).with_session_service(session_service);

                    let loaded = executor.load_chat_session(&session.id).unwrap();

                    assert_eq!(loaded.id, session.id);
                    assert_eq!(loaded.messages.len(), 1);
                }

                #[test]
                fn test_build_subagent_manager_attaches_shared_orchestrator() {
                    let (storage, _temp_dir) = create_test_storage();
                    let executor = create_test_executor(storage);
                    let manager = executor.build_subagent_manager(
                        Arc::new(CodexClient::new()),
                        Arc::new(ToolRegistry::new()),
                        Arc::new(DefaultLlmClientFactory::new(Default::default(), Vec::new())),
                    );

                    assert!(manager.orchestrator.is_some());
                    assert!(manager.llm_client_factory.is_some());
                }

                #[test]
                fn test_context_window_for_model() {
                    assert_eq!(
                        AgentRuntimeExecutor::context_window_for_model(ModelId::ClaudeSonnet4_5),
                        200_000
                    );
                    assert_eq!(
                        AgentRuntimeExecutor::context_window_for_model(ModelId::Gpt5),
                        128_000
                    );
                    assert_eq!(
                        AgentRuntimeExecutor::context_window_for_model(ModelId::DeepseekChat),
                        64_000
                    );
                    assert_eq!(
                        AgentRuntimeExecutor::context_window_for_model(ModelId::Gemini25Pro),
                        1_000_000
                    );
                }

                #[test]
                fn test_chat_resource_limits_disable_wall_clock_when_unset() {
                    let mapped = AgentRuntimeExecutor::chat_resource_limits(88, None);
                    assert_eq!(mapped.max_tool_calls, 88);
                    assert_eq!(mapped.max_wall_clock, Duration::ZERO);
                    assert_eq!(mapped.max_cost_usd, None);
                }

                #[test]
                fn test_chat_resource_limits_enable_wall_clock_when_set() {
                    let mapped = AgentRuntimeExecutor::chat_resource_limits(99, Some(123));
                    assert_eq!(mapped.max_tool_calls, 99);
                    assert_eq!(mapped.max_wall_clock, Duration::from_secs(123));
                    assert_eq!(mapped.max_cost_usd, None);
                }

                #[test]
                fn test_apply_llm_timeout_sets_timeout_when_configured() {
                    let config = ReActAgentConfig::new("goal".to_string());
                    let config = AgentRuntimeExecutor::apply_llm_timeout(config, Some(600));
                    assert_eq!(config.llm_timeout, Some(Duration::from_secs(600)));
                }

                #[test]
                fn test_apply_llm_timeout_disables_timeout_when_unset() {
                    let config = ReActAgentConfig::new("goal".to_string())
                        .with_llm_timeout(Duration::from_secs(30));
                    let config = AgentRuntimeExecutor::apply_llm_timeout(config, None);
                    assert_eq!(config.llm_timeout, None);
                }

                #[test]
                fn test_apply_execution_context_populates_context_keys() {
                    let context = ExecutionContext::main("agent-1", "session-1");
                    let config = ReActAgentConfig::new("goal".to_string());
                    let config = AgentRuntimeExecutor::apply_execution_context(config, &context);

                    assert_eq!(
                        config.context.get("execution_role"),
                        Some(&serde_json::Value::String("main_agent".to_string()))
                    );
                    assert_eq!(config.context["chat_session_id"], "session-1");
                    assert_eq!(config.context["execution_context"]["role"], "main_agent");
                }

                #[test]
                fn test_apply_execution_context_uses_parent_run_id_for_subagent_context() {
                    let context = ExecutionContext::subagent("agent-2", "run-parent-1");
                    let config = ReActAgentConfig::new("goal".to_string());
                    let config = AgentRuntimeExecutor::apply_execution_context(config, &context);

                    assert_eq!(config.context["parent_run_id"], "run-parent-1");
                    assert_eq!(
                        config.context["execution_context"]["parent_run_id"],
                        "run-parent-1"
                    );
                }

                #[test]
                fn test_effective_max_tool_result_length_respects_small_requested_limit() {
                    let value =
                        AgentRuntimeExecutor::effective_max_tool_result_length(300, 128_000);
                    assert_eq!(value, 300);
                }

                #[test]
                fn test_effective_max_tool_result_length_clamps_large_requested_limit() {
                    let value =
                        AgentRuntimeExecutor::effective_max_tool_result_length(1_000_000, 128_000);
                    assert_eq!(value, TOOL_RESULT_MAX_CHARS);
                }

                #[test]
                fn test_effective_max_tool_result_length_for_small_context_window() {
                    let value =
                        AgentRuntimeExecutor::effective_max_tool_result_length(1_000_000, 2013);
                    assert_eq!(value, 644);
                }

                struct NoopReplySender;

                impl ReplySender for NoopReplySender {
                    fn send(
                        &self,
                        _message: String,
                    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
                    {
                        Box::pin(async { Ok(()) })
                    }
                }

                #[test]
                fn test_filter_requested_tool_names_removes_reply_without_sender() {
                    let (storage, _temp_dir) = create_test_storage();
                    let executor = create_test_executor(storage);
                    let requested =
                        vec!["bash".to_string(), "reply".to_string(), "file".to_string()];

                    let filtered = executor
                        .filter_requested_tool_names(Some(&requested), false)
                        .expect("filtered tool list");

                    assert!(filtered.iter().any(|name| name == "bash"));
                    assert!(filtered.iter().any(|name| name == "file"));
                    assert!(!filtered.iter().any(|name| name == "reply"));
                }

                #[test]
                fn test_filter_requested_tool_names_keeps_reply_with_sender() {
                    let (storage, _temp_dir) = create_test_storage();
                    let executor =
                        create_test_executor(storage).with_reply_sender(Arc::new(NoopReplySender));
                    let requested = vec!["reply".to_string(), "bash".to_string()];

                    let filtered = executor
                        .filter_requested_tool_names(Some(&requested), true)
                        .expect("filtered tool list");

                    assert!(filtered.iter().any(|name| name == "reply"));
                    assert!(filtered.iter().any(|name| name == "bash"));
                }

                #[cfg(unix)]
                #[test]
                fn test_resolve_preflight_skills_includes_team_skrun_skill() {
                    let (storage, temp_dir) = create_test_storage();
                    let bin = install_skrun_skills(
                        &temp_dir,
                        r##"[{
                          "id": "team",
                          "name": "Team",
                          "version": "0.1.0",
                          "kind": "markdown",
                          "content": "# Team\n\nUse spawn_subagent_batch.",
                          "suggested_tools": ["spawn_subagent_batch"],
                          "executable": false,
                          "source_ref": "skrun:team@0.1.0"
                        }]"##,
                    );
                    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
                    let executor = create_test_executor(storage);

                    let node = AgentNode {
                        skills: Some(vec!["team".to_string()]),
                        ..AgentNode::new()
                    };
                    let skills = executor.resolve_preflight_skills(&node, None).unwrap();

                    assert_eq!(skills.len(), 1);
                    assert_eq!(skills[0].id, "team");
                    assert_eq!(skills[0].source, SkillSource::External);
                    assert!(skills[0].read_only);
                    assert_eq!(skills[0].source_ref.as_deref(), Some("skrun:team@0.1.0"));
                    assert!(skills[0].content.contains("spawn_subagent_batch"));
                }

                #[cfg(unix)]
                #[test]
                fn test_resolve_effective_tool_names_activates_assigned_skrun_skill_tools() {
                    let (storage, temp_dir) = create_test_storage();
                    let bin = install_skrun_skills(
                        &temp_dir,
                        r##"[{
                          "id": "shell-helper",
                          "name": "Shell Helper",
                          "version": "0.1.0",
                          "kind": "markdown",
                          "content": "# Shell Helper",
                          "suggested_tools": ["bash", "reply"],
                          "executable": false
                        }]"##,
                    );
                    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
                    let executor = create_test_executor(storage);
                    let node = AgentNode {
                        skills: Some(vec!["shell-helper".to_string()]),
                        ..AgentNode::new()
                    };

                    let tools = executor
                        .resolve_effective_tool_names(&node, None, None)
                        .expect("assigned skrun skill should activate suggested tools");

                    assert!(tools.iter().any(|tool| tool == "bash"));
                    assert!(tools.iter().any(|tool| tool == "reply"));
                }

                #[cfg(unix)]
                #[test]
                fn test_resolve_effective_tool_names_activates_explicit_skill_mention() {
                    let (storage, temp_dir) = create_test_storage();
                    let bin = install_skrun_skills(
                        &temp_dir,
                        r##"[{
                          "id": "shell-helper",
                          "name": "Shell Helper",
                          "version": "0.1.0",
                          "kind": "markdown",
                          "content": "# Shell Helper",
                          "suggested_tools": ["bash", "reply"],
                          "executable": false
                        }]"##,
                    );
                    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
                    let executor = create_test_executor(storage);
                    let node = AgentNode {
                        skills: Some(vec!["shell-helper".to_string()]),
                        ..AgentNode::new()
                    };

                    let tools = executor
                        .resolve_effective_tool_names(&node, None, Some("please use @shell-helper"))
                        .expect("explicit skill mention should activate suggested tools");

                    assert!(tools.iter().any(|tool| tool == "load_skill"));
                    assert!(tools.iter().any(|tool| tool == "bash"));
                    assert!(tools.iter().any(|tool| tool == "reply"));
                }

                #[cfg(unix)]
                #[test]
                fn test_resolve_effective_tool_names_rejects_known_unassigned_skill_mention() {
                    let (storage, temp_dir) = create_test_storage();
                    let bin = install_skrun_skills(
                        &temp_dir,
                        r##"[{
                          "id": "manage-agents-skill",
                          "name": "Manage Agents",
                          "version": "0.1.0",
                          "kind": "markdown",
                          "content": "# Manage Agents",
                          "suggested_tools": ["manage_agents"],
                          "executable": false
                        }]"##,
                    );
                    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
                    let executor = create_test_executor(storage);
                    let node = AgentNode::new();

                    let tools = executor
                        .resolve_effective_tool_names(
                            &node,
                            None,
                            Some("please use @manage-agents-skill"),
                        )
                        .expect("invalid skill mentions should not fail the turn");

                    assert!(tools.iter().any(|tool| tool == "load_skill"));
                    assert!(!tools.iter().any(|tool| tool == "manage_agents"));
                }

                #[tokio::test]
                async fn test_resolve_primary_model_prefers_explicit_model() {
                    let (storage, _temp_dir) = create_test_storage();
                    let executor = create_test_executor(storage);
                    let node = AgentNode::with_model(ModelId::ClaudeSonnet4_5);

                    let resolved = executor.resolve_primary_model(&node).await.unwrap();
                    assert_eq!(resolved, ModelId::ClaudeSonnet4_5);
                }

                #[tokio::test]
                async fn test_resolve_primary_model_uses_openai_secret_when_model_missing() {
                    let (storage, _temp_dir) = create_test_storage();
                    storage
                        .secrets
                        .set_secret("OPENAI_API_KEY", "test-openai-key", None)
                        .unwrap();
                    let executor = create_test_executor(storage);
                    let node = AgentNode::new();

                    let resolved = executor.resolve_primary_model(&node).await.unwrap();
                    assert_eq!(resolved, ModelId::Gpt5_4);
                }

                #[tokio::test]
                async fn test_resolve_primary_model_uses_anthropic_opus_when_model_missing() {
                    let (storage, _temp_dir) = create_test_storage();
                    storage
                        .secrets
                        .set_secret("ANTHROPIC_API_KEY", "test-anthropic-key", None)
                        .unwrap();
                    let executor = create_test_executor(storage);
                    let node = AgentNode::new();

                    let resolved = executor.resolve_primary_model(&node).await.unwrap();
                    assert_eq!(resolved, ModelId::ClaudeOpus4_6);
                }

                #[test]
                fn test_default_model_for_provider_uses_anthropic_opus() {
                    assert_eq!(
                        provider_default_model(Provider::Anthropic),
                        ModelId::ClaudeOpus4_6
                    );
                }

                #[test]
                fn test_default_model_for_provider_uses_minimax_m27() {
                    assert_eq!(
                        provider_default_model(Provider::MiniMax),
                        ModelId::MiniMaxM27
                    );
                }

                #[tokio::test]
                async fn test_executor_no_api_key() {
                    let (storage, _temp_dir) = create_test_storage();
                    let executor = create_test_executor(storage);
                    let result = executor
                        .resolve_api_key_for_model(
                            Provider::Anthropic,
                            Some(&ApiKeyConfig::Secret("MISSING_TEST_SECRET".to_string())),
                            Provider::Anthropic,
                        )
                        .await;

                    assert!(result.is_err());
                    let err_msg = result.unwrap_err().to_string();
                    assert!(
                        err_msg.contains("MISSING_TEST_SECRET"),
                        "Error should mention missing secret: {}",
                        err_msg
                    );
                }

                #[cfg(unix)]
                #[tokio::test]
                async fn test_execute_session_turn_enforces_skill_preflight_policy() {
                    let (storage, temp_dir) = create_test_storage();
                    let executor = create_test_executor(storage.clone());
                    let bin = install_skrun_skills(
                        &temp_dir,
                        r##"[{
                          "id": "preflight-session-skill",
                          "name": "Preflight Blocking Skill",
                          "version": "0.1.0",
                          "kind": "markdown",
                          "content": "Use {{missing_input}} to proceed",
                          "suggested_tools": ["missing_tool_for_test"],
                          "executable": false
                        }]"##,
                    );
                    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);

                    let agent = AgentNode::with_model(ModelId::CodexCli)
                        .with_skills(vec!["preflight-session-skill".to_string()])
                        .with_skill_preflight_policy_mode(SkillPreflightPolicyMode::Enforce);
                    let stored_agent = storage
                        .agents
                        .create_agent("session-preflight-agent".to_string(), agent)
                        .unwrap();

                    let mut session = ChatSession::new(
                        stored_agent.id.clone(),
                        ModelId::CodexCli.as_serialized_str().to_string(),
                    );

                    let result = executor
                        .execute_session_turn_with_emitter_and_steer(
                            &mut session,
                            "run preflight check",
                            16,
                            SessionInputMode::EphemeralInput,
                            None,
                            SessionTurnRuntimeOptions::default(),
                        )
                        .await;

                    assert!(result.is_err());
                    let err = result.unwrap_err().to_string();
                    assert!(err.contains("Preflight check failed"));
                    assert!(err.contains("missing_tool"));
                    assert!(err.contains("missing_input"));
                }

                #[tokio::test]
                async fn test_resolve_api_key_requires_matching_zai_secret() {
                    let (storage, _temp_dir) = create_test_storage();
                    storage
                        .secrets
                        .set_secret("ZAI_CODING_PLAN_API_KEY", "zai-coding-plan-key", None)
                        .unwrap();
                    let executor = create_test_executor(storage);

                    let result = executor
                        .resolve_api_key_for_model(Provider::Zai, None, Provider::Zai)
                        .await;

                    assert!(result.is_err());
                }

                #[tokio::test]
                async fn test_resolve_api_key_requires_matching_zai_coding_plan_secret() {
                    let (storage, _temp_dir) = create_test_storage();
                    storage
                        .secrets
                        .set_secret("ZAI_API_KEY", "zai-key", None)
                        .unwrap();
                    let executor = create_test_executor(storage);

                    let result = executor
                        .resolve_api_key_for_model(
                            Provider::ZaiCodingPlan,
                            None,
                            Provider::ZaiCodingPlan,
                        )
                        .await;

                    assert!(result.is_err());
                }

                // Note: test_build_tool_registry removed because build_tool_registry now requires
                // an LlmClient for SubagentDeps. The core logic (registry_from_allowlist) is
                // covered by integration tests in the daemon transport stack
            }
        }
        pub mod failover {
            //! Model Failover System
            //!
            //! This module provides automatic failover between AI models when the primary
            //! model fails or becomes unavailable. It tracks model health and routes
            //! requests to healthy fallback models.
            //!
            //! # Features
            //!
            //! - Primary/fallback model configuration
            //! - Automatic health tracking per model
            //! - Cooldown periods after failures
            //! - Circuit breaker pattern for unhealthy models
            //! - Configurable failure thresholds
            //!
            //! # Example
            //!
            //! ```ignore
            //! use runner::runtime::session_runner::failover::{FailoverConfig, FailoverManager};
            //! use crate::ModelId;
            //!
            //! let config = FailoverConfig {
            //!     primary: ModelId::ClaudeSonnet4_5,
            //!     fallbacks: vec![ModelId::Gpt5, ModelId::DeepseekChat],
            //!     cooldown_secs: 300,
            //!     failure_threshold: 3,
            //! };
            //!
            //! let manager = FailoverManager::new(config);
            //!
            //! // Get the best available model
            //! if let Some(model) = manager.get_available_model().await {
            //!     // Use this model for the request
            //! }
            //!
            //! // Record failure/success
            //! manager.record_failure(ModelId::ClaudeSonnet4_5).await;
            //! manager.record_success(ModelId::Gpt5).await;
            //! ```

            use crate::{ModelId, Provider};
            use anyhow::Result;
            use serde::{Deserialize, Serialize};
            use std::collections::{HashMap, HashSet};
            use std::sync::Arc;
            use tokio::sync::RwLock;
            use tracing::{debug, info, warn};

            use super::error_classification::{
                classify_execution_error_message, is_authentication_classification,
            };

            /// Configuration for the model failover system
            #[derive(Debug, Clone, Serialize, Deserialize)]
            pub struct FailoverConfig {
                /// Primary model to use when healthy
                pub primary: ModelId,
                /// Fallback models in order of preference
                pub fallbacks: Vec<ModelId>,
                /// Cooldown period in seconds after a model failure
                pub cooldown_secs: u64,
                /// Number of consecutive failures before putting model in cooldown
                pub failure_threshold: u32,
                /// Whether to automatically recover models after cooldown expires
                pub auto_recover: bool,
            }

            impl Default for FailoverConfig {
                fn default() -> Self {
                    Self {
                        primary: ModelId::ClaudeSonnet4_5,
                        fallbacks: vec![ModelId::Gpt5, ModelId::DeepseekChat],
                        cooldown_secs: 300,   // 5 minutes
                        failure_threshold: 3, // 3 consecutive failures
                        auto_recover: true,
                    }
                }
            }

            impl FailoverConfig {
                /// Create a new failover config with the specified primary model.
                ///
                /// CLI-based models (Codex CLI, OpenCode, Gemini CLI) disable fallbacks
                /// because they manage their own authentication and cannot fall back
                /// to API-based models that require different credentials.
                pub fn with_primary(primary: ModelId) -> Self {
                    let fallbacks = if primary.is_cli_model() {
                        vec![]
                    } else if primary == ModelId::ClaudeOpus4_6 {
                        vec![ModelId::ClaudeSonnet4_5]
                    } else {
                        Self::default().fallbacks
                    };
                    Self {
                        primary,
                        fallbacks,
                        ..Default::default()
                    }
                }

                /// Create a config with custom fallbacks
                pub fn with_fallbacks(primary: ModelId, fallbacks: Vec<ModelId>) -> Self {
                    Self {
                        primary,
                        fallbacks,
                        ..Default::default()
                    }
                }

                /// Build a failover chain that only includes models with available credentials.
                ///
                /// Priority order:
                /// 1. Same-provider downgrade (e.g., glm-5 -> glm-5-turbo -> glm-5-code -> glm-4-7)
                /// 2. Manually configured cross-provider fallbacks (from config)
                ///
                /// Note: Automatic cross-provider failover has been removed.
                /// Users must manually configure fallback models via config.toml.
                pub fn build_smart(
                    primary: ModelId,
                    _available_providers: &HashSet<Provider>,
                    manual_fallbacks: Option<Vec<ModelId>>,
                ) -> Self {
                    if primary.is_cli_model() {
                        return Self {
                            primary,
                            fallbacks: vec![],
                            ..Default::default()
                        };
                    }

                    let mut fallbacks = Vec::new();
                    let mut seen = HashSet::new();
                    seen.insert(primary);

                    // 1. Same-provider downgrade chain (always automatic)
                    let mut current = primary;
                    while let Some(fallback) = current.same_provider_fallback() {
                        if seen.insert(fallback) {
                            fallbacks.push(fallback);
                        }
                        current = fallback;
                    }

                    // 2. Manually configured cross-provider fallbacks (from config)
                    if let Some(manual) = manual_fallbacks {
                        for model in manual {
                            if seen.insert(model) {
                                fallbacks.push(model);
                            }
                        }
                    }

                    Self {
                        primary,
                        fallbacks,
                        ..Default::default()
                    }
                }

                /// Get all models in priority order (primary first, then fallbacks)
                pub fn all_models(&self) -> Vec<ModelId> {
                    let mut models = vec![self.primary];
                    models.extend(self.fallbacks.iter().copied());
                    models
                }

                /// Check if a model is in the failover chain
                pub fn contains(&self, model: ModelId) -> bool {
                    self.primary == model || self.fallbacks.contains(&model)
                }
            }

            /// Health state for a single model
            #[derive(Debug, Clone, Default)]
            struct ModelHealth {
                /// Number of consecutive failures
                consecutive_failures: u32,
                /// Total failures since last reset
                total_failures: u32,
                /// Total successes since last reset
                total_successes: u32,
                /// Timestamp when cooldown expires (None = healthy)
                cooldown_until: Option<i64>,
                /// Last failure error message
                last_error: Option<String>,
                /// Timestamp of last failure
                last_failure_at: Option<i64>,
                /// Timestamp of last success
                last_success_at: Option<i64>,
            }

            impl ModelHealth {
                fn new() -> Self {
                    Self::default()
                }

                /// Check if the model is currently in cooldown
                fn is_in_cooldown(&self, now: i64) -> bool {
                    self.cooldown_until
                        .map(|until| now < until)
                        .unwrap_or(false)
                }

                /// Check if the model is available (not in cooldown)
                fn is_available(&self, now: i64) -> bool {
                    !self.is_in_cooldown(now)
                }

                /// Get remaining cooldown time in milliseconds
                fn remaining_cooldown_ms(&self, now: i64) -> Option<i64> {
                    self.cooldown_until.and_then(|until| {
                        let remaining = until - now;
                        if remaining > 0 { Some(remaining) } else { None }
                    })
                }

                /// Calculate success rate (0.0 to 1.0)
                fn success_rate(&self) -> f64 {
                    let total = self.total_successes + self.total_failures;
                    if total == 0 {
                        1.0 // Assume healthy if no data
                    } else {
                        self.total_successes as f64 / total as f64
                    }
                }
            }

            /// Model status information for external use
            #[derive(Debug, Clone, Serialize, Deserialize)]
            pub struct ModelStatus {
                pub model: ModelId,
                pub available: bool,
                pub consecutive_failures: u32,
                pub success_rate: f64,
                pub cooldown_remaining_secs: Option<u64>,
                pub last_error: Option<String>,
            }

            /// The failover manager that tracks model health and selects available models
            pub struct FailoverManager {
                config: FailoverConfig,
                health: Arc<RwLock<HashMap<ModelId, ModelHealth>>>,
            }

            impl FailoverManager {
                /// Create a new failover manager with the given configuration
                pub fn new(config: FailoverConfig) -> Self {
                    Self {
                        config,
                        health: Arc::new(RwLock::new(HashMap::new())),
                    }
                }

                /// Create a manager with default configuration
                pub fn with_defaults() -> Self {
                    Self::new(FailoverConfig::default())
                }

                /// Get the best available model
                ///
                /// Returns the primary model if healthy, otherwise the first healthy fallback.
                /// Returns None if all models are in cooldown.
                pub async fn get_available_model(&self) -> Option<ModelId> {
                    let health = self.health.read().await;
                    let now = chrono::Utc::now().timestamp_millis();

                    // Try primary first
                    if self.is_model_available(&health, self.config.primary, now) {
                        return Some(self.config.primary);
                    }

                    debug!(
                        "Primary model {:?} unavailable, checking fallbacks",
                        self.config.primary
                    );

                    // Try fallbacks in order
                    for &model in &self.config.fallbacks {
                        if self.is_model_available(&health, model, now) {
                            info!("Failing over to model {:?}", model);
                            return Some(model);
                        }
                    }

                    warn!("All models are in cooldown or unavailable");
                    None
                }

                /// Get a specific model if available, or the best alternative
                pub async fn get_model_or_fallback(&self, preferred: ModelId) -> Option<ModelId> {
                    let health = self.health.read().await;
                    let now = chrono::Utc::now().timestamp_millis();

                    // Try preferred model first
                    if self.is_model_available(&health, preferred, now) {
                        return Some(preferred);
                    }

                    // Fall back to normal priority order
                    drop(health);
                    self.get_available_model().await
                }

                /// Check if a specific model is available
                fn is_model_available(
                    &self,
                    health: &HashMap<ModelId, ModelHealth>,
                    model: ModelId,
                    now: i64,
                ) -> bool {
                    match health.get(&model) {
                        Some(h) => h.is_available(now),
                        None => true, // No health data = assume healthy
                    }
                }

                /// Record a successful request to a model
                pub async fn record_success(&self, model: ModelId) {
                    let mut health = self.health.write().await;
                    let now = chrono::Utc::now().timestamp_millis();

                    let entry = health.entry(model).or_insert_with(ModelHealth::new);
                    entry.consecutive_failures = 0;
                    entry.total_successes += 1;
                    entry.last_success_at = Some(now);

                    // Clear cooldown on success (if auto_recover is enabled)
                    if self.config.auto_recover {
                        entry.cooldown_until = None;
                    }

                    debug!(
                        "Model {:?} success recorded (total: {}, rate: {:.1}%)",
                        model,
                        entry.total_successes,
                        entry.success_rate() * 100.0
                    );
                }

                /// Record a failed request to a model
                pub async fn record_failure(&self, model: ModelId) {
                    self.record_failure_with_error(model, None).await
                }

                /// Record a failed request with error details
                pub async fn record_failure_with_error(&self, model: ModelId, error: Option<&str>) {
                    let mut health = self.health.write().await;
                    let now = chrono::Utc::now().timestamp_millis();

                    let entry = health.entry(model).or_insert_with(ModelHealth::new);
                    entry.consecutive_failures += 1;
                    entry.total_failures += 1;
                    entry.last_failure_at = Some(now);

                    if let Some(err) = error {
                        entry.last_error = Some(err.to_string());
                    }

                    // Check if we should put the model in cooldown
                    if entry.consecutive_failures >= self.config.failure_threshold {
                        let cooldown_until = now + (self.config.cooldown_secs * 1000) as i64;
                        entry.cooldown_until = Some(cooldown_until);

                        warn!(
                            "Model {:?} placed in cooldown for {}s after {} consecutive failures",
                            model, self.config.cooldown_secs, entry.consecutive_failures
                        );
                    } else {
                        debug!(
                            "Model {:?} failure {}/{} before cooldown",
                            model, entry.consecutive_failures, self.config.failure_threshold
                        );
                    }
                }

                /// Manually clear cooldown for a model
                pub async fn clear_cooldown(&self, model: ModelId) {
                    let mut health = self.health.write().await;
                    if let Some(entry) = health.get_mut(&model) {
                        entry.cooldown_until = None;
                        entry.consecutive_failures = 0;
                        info!("Manually cleared cooldown for model {:?}", model);
                    }
                }

                /// Manually put a model in cooldown
                pub async fn force_cooldown(&self, model: ModelId) {
                    let mut health = self.health.write().await;
                    let now = chrono::Utc::now().timestamp_millis();
                    let cooldown_until = now + (self.config.cooldown_secs * 1000) as i64;

                    let entry = health.entry(model).or_insert_with(ModelHealth::new);
                    entry.cooldown_until = Some(cooldown_until);

                    info!(
                        "Manually placed model {:?} in cooldown for {}s",
                        model, self.config.cooldown_secs
                    );
                }

                /// Get the status of all configured models
                pub async fn get_all_status(&self) -> Vec<ModelStatus> {
                    let health = self.health.read().await;
                    let now = chrono::Utc::now().timestamp_millis();

                    self.config
                        .all_models()
                        .into_iter()
                        .map(|model| self.model_status(&health, model, now))
                        .collect()
                }

                /// Get the status of a specific model
                pub async fn get_status(&self, model: ModelId) -> ModelStatus {
                    let health = self.health.read().await;
                    let now = chrono::Utc::now().timestamp_millis();
                    self.model_status(&health, model, now)
                }

                fn model_status(
                    &self,
                    health: &HashMap<ModelId, ModelHealth>,
                    model: ModelId,
                    now: i64,
                ) -> ModelStatus {
                    match health.get(&model) {
                        Some(h) => ModelStatus {
                            model,
                            available: h.is_available(now),
                            consecutive_failures: h.consecutive_failures,
                            success_rate: h.success_rate(),
                            cooldown_remaining_secs: h
                                .remaining_cooldown_ms(now)
                                .map(|ms| (ms / 1000) as u64),
                            last_error: h.last_error.clone(),
                        },
                        None => ModelStatus {
                            model,
                            available: true,
                            consecutive_failures: 0,
                            success_rate: 1.0,
                            cooldown_remaining_secs: None,
                            last_error: None,
                        },
                    }
                }

                /// Reset all health tracking data
                pub async fn reset(&self) {
                    let mut health = self.health.write().await;
                    health.clear();
                    info!("Failover manager reset - all models marked healthy");
                }

                /// Get the current configuration
                pub fn config(&self) -> &FailoverConfig {
                    &self.config
                }

                /// Check if any model is available
                pub async fn any_available(&self) -> bool {
                    self.get_available_model().await.is_some()
                }

                /// Get count of available models
                pub async fn available_count(&self) -> usize {
                    let health = self.health.read().await;
                    let now = chrono::Utc::now().timestamp_millis();

                    self.config
                        .all_models()
                        .iter()
                        .filter(|&&model| self.is_model_available(&health, model, now))
                        .count()
                }
            }

            /// Check if an error is an authentication/credential error (non-retryable for this model).
            fn is_auth_error(error: &str) -> bool {
                is_authentication_classification(classify_execution_error_message(error))
            }

            /// Execute a task with automatic failover
            ///
            /// Tries the primary model first, then falls back to alternates on failure.
            /// Returns the result from the first successful model or the last error.
            ///
            /// Auth errors (missing API key, 401, 403) immediately skip to the next model
            /// without counting toward the failure threshold.
            pub async fn execute_with_failover<F, Fut, T>(
                manager: &FailoverManager,
                mut execute_fn: F,
            ) -> Result<(T, ModelId)>
            where
                F: FnMut(ModelId) -> Fut,
                Fut: std::future::Future<Output = Result<T>>,
            {
                let models = manager.config().all_models();
                let mut last_error = None;

                for model in models {
                    // Check if this model is available
                    let status = manager.get_status(model).await;
                    if !status.available {
                        debug!("Skipping model {:?} (in cooldown)", model);
                        continue;
                    }

                    debug!("Attempting execution with model {:?}", model);

                    match execute_fn(model).await {
                        Ok(result) => {
                            manager.record_success(model).await;
                            return Ok((result, model));
                        }
                        Err(e) => {
                            let error_str = e.to_string();
                            if is_auth_error(&error_str) {
                                // Auth errors: immediately put in cooldown, don't count toward threshold
                                warn!("Model {:?} auth error (skipping): {}", model, error_str);
                                manager.force_cooldown(model).await;
                            } else {
                                warn!("Model {:?} failed: {}", model, error_str);
                                manager
                                    .record_failure_with_error(model, Some(&error_str))
                                    .await;
                            }
                            last_error = Some(e);
                        }
                    }
                }

                // All models failed
                Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No models available")))
            }

            #[cfg(test)]
            mod tests {
                use super::*;

                fn test_config() -> FailoverConfig {
                    FailoverConfig {
                        primary: ModelId::ClaudeSonnet4_5,
                        fallbacks: vec![ModelId::Gpt5, ModelId::DeepseekChat],
                        cooldown_secs: 60,
                        failure_threshold: 2,
                        auto_recover: true,
                    }
                }

                #[tokio::test]
                async fn test_get_available_model_primary_healthy() {
                    let manager = FailoverManager::new(test_config());

                    let model = manager.get_available_model().await;
                    assert_eq!(model, Some(ModelId::ClaudeSonnet4_5));
                }

                #[tokio::test]
                async fn test_get_available_model_primary_in_cooldown() {
                    let manager = FailoverManager::new(test_config());

                    // Put primary in cooldown
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;

                    let model = manager.get_available_model().await;
                    // Should fall back to first fallback
                    assert_eq!(model, Some(ModelId::Gpt5));
                }

                #[tokio::test]
                async fn test_get_available_model_all_in_cooldown() {
                    let config = FailoverConfig {
                        primary: ModelId::ClaudeSonnet4_5,
                        fallbacks: vec![ModelId::Gpt5],
                        cooldown_secs: 60,
                        failure_threshold: 1, // Single failure triggers cooldown
                        auto_recover: true,
                    };
                    let manager = FailoverManager::new(config);

                    // Put all models in cooldown
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    manager.record_failure(ModelId::Gpt5).await;

                    let model = manager.get_available_model().await;
                    assert_eq!(model, None);
                }

                #[tokio::test]
                async fn test_record_success_clears_cooldown() {
                    let manager = FailoverManager::new(test_config());

                    // Put in cooldown
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;

                    // Verify in cooldown
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(!status.available);

                    // Record success
                    manager.record_success(ModelId::ClaudeSonnet4_5).await;

                    // Should be available again
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(status.available);
                }

                #[tokio::test]
                async fn test_failure_threshold() {
                    let config = FailoverConfig {
                        primary: ModelId::ClaudeSonnet4_5,
                        fallbacks: vec![],
                        cooldown_secs: 60,
                        failure_threshold: 3,
                        auto_recover: true,
                    };
                    let manager = FailoverManager::new(config);

                    // First two failures: still available
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(status.available);
                    assert_eq!(status.consecutive_failures, 1);

                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(status.available);
                    assert_eq!(status.consecutive_failures, 2);

                    // Third failure: should trigger cooldown
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(!status.available);
                    assert_eq!(status.consecutive_failures, 3);
                }

                #[tokio::test]
                async fn test_clear_cooldown() {
                    let manager = FailoverManager::new(test_config());

                    // Put in cooldown
                    manager.force_cooldown(ModelId::ClaudeSonnet4_5).await;

                    // Verify in cooldown
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(!status.available);

                    // Clear cooldown
                    manager.clear_cooldown(ModelId::ClaudeSonnet4_5).await;

                    // Should be available
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(status.available);
                }

                #[tokio::test]
                async fn test_get_all_status() {
                    let manager = FailoverManager::new(test_config());

                    let statuses = manager.get_all_status().await;
                    assert_eq!(statuses.len(), 3); // primary + 2 fallbacks

                    // All should be available initially
                    for status in &statuses {
                        assert!(status.available);
                        assert_eq!(status.consecutive_failures, 0);
                    }
                }

                #[tokio::test]
                async fn test_success_rate() {
                    let manager = FailoverManager::new(test_config());

                    // 3 successes, 1 failure = 75% success rate
                    manager.record_success(ModelId::ClaudeSonnet4_5).await;
                    manager.record_success(ModelId::ClaudeSonnet4_5).await;
                    manager.record_success(ModelId::ClaudeSonnet4_5).await;
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;

                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!((status.success_rate - 0.75).abs() < 0.01);
                }

                #[tokio::test]
                async fn test_reset() {
                    let manager = FailoverManager::new(test_config());

                    // Add some state
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    manager.record_failure(ModelId::ClaudeSonnet4_5).await;
                    manager.record_failure(ModelId::Gpt5).await;

                    // Reset
                    manager.reset().await;

                    // All models should be healthy
                    let statuses = manager.get_all_status().await;
                    for status in statuses {
                        assert!(status.available);
                        assert_eq!(status.consecutive_failures, 0);
                        assert_eq!(status.success_rate, 1.0);
                    }
                }

                #[tokio::test]
                async fn test_available_count() {
                    let manager = FailoverManager::new(test_config());

                    // Initially all available
                    assert_eq!(manager.available_count().await, 3);

                    // Put one in cooldown
                    manager.force_cooldown(ModelId::ClaudeSonnet4_5).await;
                    assert_eq!(manager.available_count().await, 2);

                    // Put another in cooldown
                    manager.force_cooldown(ModelId::Gpt5).await;
                    assert_eq!(manager.available_count().await, 1);
                }

                #[tokio::test]
                async fn test_config_all_models() {
                    let config = test_config();
                    let models = config.all_models();

                    assert_eq!(models.len(), 3);
                    assert_eq!(models[0], ModelId::ClaudeSonnet4_5);
                    assert_eq!(models[1], ModelId::Gpt5);
                    assert_eq!(models[2], ModelId::DeepseekChat);
                }

                #[tokio::test]
                async fn test_config_contains() {
                    let config = test_config();

                    assert!(config.contains(ModelId::ClaudeSonnet4_5));
                    assert!(config.contains(ModelId::Gpt5));
                    assert!(config.contains(ModelId::DeepseekChat));
                    assert!(!config.contains(ModelId::Gemini25Pro));
                }

                #[tokio::test]
                async fn test_execute_with_failover_success() {
                    let manager = FailoverManager::new(test_config());

                    let result = execute_with_failover(&manager, |model| async move {
                        if model == ModelId::ClaudeSonnet4_5 {
                            Ok("success")
                        } else {
                            Err(anyhow::anyhow!("wrong model"))
                        }
                    })
                    .await;

                    assert!(result.is_ok());
                    let (value, model) = result.unwrap();
                    assert_eq!(value, "success");
                    assert_eq!(model, ModelId::ClaudeSonnet4_5);
                }

                #[tokio::test]
                async fn test_execute_with_failover_fallback() {
                    let manager = FailoverManager::new(test_config());

                    // Primary fails, fallback succeeds
                    let result = execute_with_failover(&manager, |model| async move {
                        if model == ModelId::Gpt5 {
                            Ok("fallback success")
                        } else {
                            Err(anyhow::anyhow!("primary failed"))
                        }
                    })
                    .await;

                    assert!(result.is_ok());
                    let (value, model) = result.unwrap();
                    assert_eq!(value, "fallback success");
                    assert_eq!(model, ModelId::Gpt5);
                }

                #[tokio::test]
                async fn test_execute_with_failover_all_fail() {
                    let manager = FailoverManager::new(test_config());

                    let result: Result<(String, ModelId)> =
                        execute_with_failover(&manager, |_model| async move {
                            Err(anyhow::anyhow!("all models fail"))
                        })
                        .await;

                    assert!(result.is_err());
                }

                #[tokio::test]
                async fn test_with_primary_claude_opus_uses_sonnet_fallback() {
                    let config = FailoverConfig::with_primary(ModelId::ClaudeOpus4_6);
                    assert_eq!(config.primary, ModelId::ClaudeOpus4_6);
                    assert_eq!(config.fallbacks, vec![ModelId::ClaudeSonnet4_5]);
                }

                #[test]
                fn test_build_smart_single_provider() {
                    let mut providers = HashSet::new();
                    providers.insert(Provider::Anthropic);

                    let config =
                        FailoverConfig::build_smart(ModelId::ClaudeOpus4_6, &providers, None);
                    assert_eq!(config.primary, ModelId::ClaudeOpus4_6);
                    // Should include same-provider downgrades only
                    assert!(config.fallbacks.contains(&ModelId::ClaudeSonnet4_5));
                    assert!(config.fallbacks.contains(&ModelId::ClaudeHaiku4_5));
                    // Should NOT include models from other providers
                    assert!(!config.fallbacks.contains(&ModelId::Gpt5));
                    assert!(!config.fallbacks.contains(&ModelId::DeepseekChat));
                }

                #[test]
                fn test_build_smart_with_manual_fallback() {
                    let mut providers = HashSet::new();
                    providers.insert(Provider::Anthropic);
                    providers.insert(Provider::OpenRouter);

                    // Test with manual fallback configuration
                    let manual_fallbacks = vec![ModelId::OrClaudeOpus4_6, ModelId::Gpt5];
                    let config = FailoverConfig::build_smart(
                        ModelId::ClaudeSonnet4_5,
                        &providers,
                        Some(manual_fallbacks),
                    );
                    assert_eq!(config.primary, ModelId::ClaudeSonnet4_5);
                    // Should include same-provider downgrade
                    assert!(config.fallbacks.contains(&ModelId::ClaudeHaiku4_5));
                    // Should include manually configured fallbacks
                    assert!(config.fallbacks.contains(&ModelId::OrClaudeOpus4_6));
                    assert!(config.fallbacks.contains(&ModelId::Gpt5));
                }

                #[test]
                fn test_build_smart_multiple_providers() {
                    let mut providers = HashSet::new();
                    providers.insert(Provider::Anthropic);
                    providers.insert(Provider::Zai);
                    providers.insert(Provider::OpenRouter);

                    // Test with manual fallbacks (automatic cross-provider fallback disabled)
                    let manual_fallbacks = vec![ModelId::Glm5, ModelId::OrClaudeOpus4_6];
                    let config = FailoverConfig::build_smart(
                        ModelId::ClaudeSonnet4_5,
                        &providers,
                        Some(manual_fallbacks),
                    );
                    assert_eq!(config.primary, ModelId::ClaudeSonnet4_5);
                    // Same-provider downgrade
                    assert!(config.fallbacks.contains(&ModelId::ClaudeHaiku4_5));
                    // Manually configured fallbacks
                    assert!(config.fallbacks.contains(&ModelId::Glm5));
                    assert!(config.fallbacks.contains(&ModelId::OrClaudeOpus4_6));
                }

                #[test]
                fn test_build_smart_cli_model_no_fallbacks() {
                    let mut providers = HashSet::new();
                    providers.insert(Provider::Anthropic);
                    providers.insert(Provider::OpenAI);

                    let config = FailoverConfig::build_smart(ModelId::CodexCli, &providers, None);
                    assert_eq!(config.primary, ModelId::CodexCli);
                    assert!(config.fallbacks.is_empty());
                }

                #[test]
                fn test_build_smart_no_duplicates() {
                    let mut providers = HashSet::new();
                    providers.insert(Provider::Anthropic);
                    providers.insert(Provider::OpenAI);
                    providers.insert(Provider::OpenRouter);
                    providers.insert(Provider::DeepSeek);

                    let config =
                        FailoverConfig::build_smart(ModelId::ClaudeOpus4_6, &providers, None);
                    let mut seen = HashSet::new();
                    seen.insert(config.primary);
                    for model in &config.fallbacks {
                        assert!(
                            seen.insert(*model),
                            "Duplicate model {:?} in fallback chain",
                            model
                        );
                    }
                }

                #[test]
                fn test_is_auth_error_detection() {
                    assert!(is_auth_error("No API key configured for provider"));
                    assert!(is_auth_error("api_key is missing"));
                    assert!(is_auth_error("Unauthorized access"));
                    assert!(is_auth_error("authentication failed"));
                    assert!(is_auth_error("Secret 'OPENAI_API_KEY' not found"));
                    assert!(is_auth_error("HTTP 401 error"));
                    assert!(is_auth_error("HTTP 403 forbidden"));
                }

                #[test]
                fn test_is_auth_error_false_for_transient() {
                    assert!(!is_auth_error("connection timeout"));
                    assert!(!is_auth_error("rate limit exceeded"));
                    assert!(!is_auth_error("internal server error"));
                    assert!(!is_auth_error("context window exceeded"));
                    assert!(!is_auth_error("model overloaded"));
                }

                #[tokio::test]
                async fn test_execute_with_failover_auth_error_skips() {
                    let config = FailoverConfig {
                        primary: ModelId::ClaudeSonnet4_5,
                        fallbacks: vec![ModelId::Gpt5, ModelId::DeepseekChat],
                        cooldown_secs: 60,
                        failure_threshold: 3,
                        auto_recover: true,
                    };
                    let manager = FailoverManager::new(config);

                    let result = execute_with_failover(&manager, |model| async move {
                        if model == ModelId::ClaudeSonnet4_5 {
                            Err(anyhow::anyhow!("No API key configured for provider"))
                        } else if model == ModelId::Gpt5 {
                            Err(anyhow::anyhow!("Unauthorized"))
                        } else {
                            Ok("success from deepseek")
                        }
                    })
                    .await;

                    assert!(result.is_ok());
                    let (value, model) = result.unwrap();
                    assert_eq!(value, "success from deepseek");
                    assert_eq!(model, ModelId::DeepseekChat);

                    // Auth-failed models should be in cooldown (not just failure-counted)
                    let status = manager.get_status(ModelId::ClaudeSonnet4_5).await;
                    assert!(!status.available);
                    let status = manager.get_status(ModelId::Gpt5).await;
                    assert!(!status.available);
                }
            }
        }
        pub mod retry {
            //! Retry manager for failed agent operations.
            //!
            //! This module provides a retry mechanism for agent operations that fail due to
            //! transient errors (e.g., network timeouts, rate limits, temporary service
            //! unavailability).
            //!
            //! # Features
            //!
            //! - Configurable maximum retry attempts
            //! - Exponential backoff with jitter
            //! - Transient error detection
            //! - Per-task retry state tracking
            //!
            //! # Example
            //!
            //! ```ignore
            //! use runner::runtime::session_runner::retry::{RetryConfig, RetryState};
            //!
            //! let config = RetryConfig::default();
            //! let mut state = RetryState::new();
            //!
            //! // After a failure
            //! if state.should_retry(&config, "Connection timeout") {
            //!     state.record_failure("Connection timeout", &config);
            //!     // Wait for state.next_retry_at before retrying
            //! }
            //! ```

            use serde::{Deserialize, Serialize};
            use std::time::Duration;

            use super::ExecutionErrorKind;
            use super::error_classification::{
                classify_execution_error_message, is_retryable_classification,
            };

            /// Configuration for the retry mechanism
            #[derive(Debug, Clone, Serialize, Deserialize)]
            pub struct RetryConfig {
                /// Maximum number of retry attempts (0 = no retries)
                pub max_retries: u32,
                /// Initial delay between retries in seconds
                pub initial_delay_secs: u64,
                /// Maximum delay between retries in seconds (caps exponential growth)
                pub max_delay_secs: u64,
                /// Multiplier for exponential backoff (e.g., 2.0 = double each time)
                pub backoff_multiplier: f64,
                /// Whether to add random jitter to delays (recommended for distributed systems)
                pub jitter_enabled: bool,
                /// Maximum jitter as a fraction of delay (e.g., 0.25 = up to 25% variation)
                pub jitter_factor: f64,
            }

            impl Default for RetryConfig {
                fn default() -> Self {
                    Self {
                        max_retries: 3,
                        initial_delay_secs: 60,  // 1 minute
                        max_delay_secs: 3600,    // 1 hour
                        backoff_multiplier: 2.0, // Double each time
                        jitter_enabled: true,
                        jitter_factor: 0.25, // Up to 25% variation
                    }
                }
            }

            impl RetryConfig {
                /// Create a new configuration with custom settings
                pub fn new(max_retries: u32, initial_delay_secs: u64) -> Self {
                    Self {
                        max_retries,
                        initial_delay_secs,
                        ..Default::default()
                    }
                }

                /// Create a configuration with no retries
                pub fn no_retries() -> Self {
                    Self {
                        max_retries: 0,
                        ..Default::default()
                    }
                }

                /// Create an aggressive retry configuration (for critical tasks)
                pub fn aggressive() -> Self {
                    Self {
                        max_retries: 5,
                        initial_delay_secs: 30,
                        max_delay_secs: 1800, // 30 minutes
                        backoff_multiplier: 1.5,
                        jitter_enabled: true,
                        jitter_factor: 0.2,
                    }
                }

                /// Create a conservative retry configuration (for less critical tasks)
                pub fn conservative() -> Self {
                    Self {
                        max_retries: 2,
                        initial_delay_secs: 120, // 2 minutes
                        max_delay_secs: 7200,    // 2 hours
                        backoff_multiplier: 3.0,
                        jitter_enabled: true,
                        jitter_factor: 0.3,
                    }
                }
            }

            /// State for tracking retry attempts for a task
            #[derive(Debug, Clone, Default, Serialize, Deserialize)]
            pub struct RetryState {
                /// Current retry attempt number (0 = initial attempt, 1 = first retry, etc.)
                pub attempt: u32,
                /// Error message from the last failure
                pub last_error: Option<String>,
                /// Timestamp (milliseconds since epoch) for when the next retry should occur
                pub next_retry_at: Option<i64>,
                /// Timestamp of the last failure
                pub last_failure_at: Option<i64>,
                /// Total number of failures (including non-retryable ones)
                pub total_failures: u32,
            }

            impl RetryState {
                /// Create a new retry state
                pub fn new() -> Self {
                    Self::default()
                }

                /// Check if we should retry based on the config and error
                ///
                /// Returns true if:
                /// - We haven't exceeded max_retries
                /// - The error is transient (retryable)
                pub fn should_retry(&self, config: &RetryConfig, error: &str) -> bool {
                    if self.attempt >= config.max_retries {
                        return false;
                    }
                    is_transient_error(error)
                }

                /// Calculate the delay before the next retry attempt
                ///
                /// Uses exponential backoff with optional jitter
                pub fn calculate_delay(&self, config: &RetryConfig) -> Duration {
                    // Base delay with exponential backoff
                    let base_delay = config.initial_delay_secs as f64
                        * config.backoff_multiplier.powi(self.attempt as i32);

                    // Cap at maximum delay
                    let capped_delay = base_delay.min(config.max_delay_secs as f64);

                    // Add jitter if enabled
                    let final_delay = if config.jitter_enabled {
                        let jitter_range = capped_delay * config.jitter_factor;
                        // Simple deterministic jitter based on attempt number
                        // In production, you might want to use actual random jitter
                        let jitter = jitter_range * ((self.attempt as f64 * 0.37).sin().abs());
                        capped_delay + jitter
                    } else {
                        capped_delay
                    };

                    Duration::from_secs(final_delay as u64)
                }

                /// Record a failure and update retry state
                ///
                /// Increments the attempt counter and calculates the next retry time
                pub fn record_failure(&mut self, error: &str, config: &RetryConfig) {
                    let now = chrono::Utc::now().timestamp_millis();

                    self.attempt += 1;
                    self.total_failures += 1;
                    self.last_error = Some(error.to_string());
                    self.last_failure_at = Some(now);

                    // Calculate next retry time if we haven't exceeded max retries
                    if self.attempt < config.max_retries && is_transient_error(error) {
                        let delay = self.calculate_delay(config);
                        self.next_retry_at = Some(now + delay.as_millis() as i64);
                    } else {
                        self.next_retry_at = None;
                    }
                }

                /// Check if a retry is due (current time >= next_retry_at)
                pub fn is_retry_due(&self) -> bool {
                    match self.next_retry_at {
                        Some(retry_at) => {
                            let now = chrono::Utc::now().timestamp_millis();
                            now >= retry_at
                        }
                        None => false,
                    }
                }

                /// Get the remaining time until the next retry in milliseconds
                pub fn time_until_retry(&self) -> Option<i64> {
                    self.next_retry_at.map(|retry_at| {
                        let now = chrono::Utc::now().timestamp_millis();
                        (retry_at - now).max(0)
                    })
                }

                /// Reset the retry state (e.g., after a successful execution)
                pub fn reset(&mut self) {
                    self.attempt = 0;
                    self.last_error = None;
                    self.next_retry_at = None;
                    self.last_failure_at = None;
                    // Note: total_failures is preserved for historical tracking
                }

                /// Check if we've exhausted all retry attempts
                pub fn is_exhausted(&self, config: &RetryConfig) -> bool {
                    self.attempt >= config.max_retries
                }

                /// Get a human-readable status string
                pub fn status_string(&self, config: &RetryConfig) -> String {
                    if self.attempt == 0 {
                        return "Not retried".to_string();
                    }

                    if self.is_exhausted(config) {
                        return format!(
                            "Exhausted ({}/{} retries, {} total failures)",
                            self.attempt, config.max_retries, self.total_failures
                        );
                    }

                    match self.time_until_retry() {
                        Some(ms) if ms > 0 => {
                            let secs = ms / 1000;
                            format!(
                                "Retry {}/{} in {}s",
                                self.attempt + 1,
                                config.max_retries,
                                secs
                            )
                        }
                        _ => format!("Retry {}/{} ready", self.attempt + 1, config.max_retries),
                    }
                }
            }

            /// Determine if an error is transient and worth retrying
            ///
            /// Transient errors are temporary failures that might succeed on retry:
            /// - Network timeouts
            /// - Connection errors
            /// - Rate limiting (429, 503)
            /// - Temporary service unavailability
            ///
            /// Non-transient errors should not be retried:
            /// - Authentication failures (401, 403)
            /// - Bad requests (400)
            /// - Not found (404)
            /// - Configuration errors
            ///
            /// Prefer using `AiError::is_retryable()` when the original error type is available.
            /// This string-based check is a fallback for contexts where only the error message is available.
            pub fn is_transient_error(error: &str) -> bool {
                let classification = classify_execution_error_message(error);
                is_retryable_classification(classification)
                    && matches!(
                        classification.kind,
                        ExecutionErrorKind::RateLimited | ExecutionErrorKind::Timeout
                    )
            }

            /// Categorize an error for logging and metrics
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum ErrorCategory {
                /// Temporary failure, worth retrying
                Transient,
                /// Authentication/authorization failure
                AuthError,
                /// Client-side error (bad request, validation)
                ClientError,
                /// Resource not found
                NotFound,
                /// Unknown error category
                Unknown,
            }

            impl ErrorCategory {
                /// Categorize an error message
                pub fn from_error(error: &str) -> Self {
                    let error_lower = error.to_lowercase();
                    if error_lower.contains("404") || error_lower.contains("not found") {
                        return Self::NotFound;
                    }

                    match classify_execution_error_message(error).kind {
                        ExecutionErrorKind::Authentication => Self::AuthError,
                        ExecutionErrorKind::RateLimited | ExecutionErrorKind::Timeout => {
                            Self::Transient
                        }
                        ExecutionErrorKind::Validation => Self::ClientError,
                        _ => Self::Unknown,
                    }
                }

                /// Whether this error category should be retried
                pub fn should_retry(&self) -> bool {
                    matches!(self, Self::Transient)
                }
            }

            #[cfg(test)]
            mod tests {
                use super::*;

                #[test]
                fn test_retry_config_default() {
                    let config = RetryConfig::default();
                    assert_eq!(config.max_retries, 3);
                    assert_eq!(config.initial_delay_secs, 60);
                    assert_eq!(config.max_delay_secs, 3600);
                    assert_eq!(config.backoff_multiplier, 2.0);
                    assert!(config.jitter_enabled);
                }

                #[test]
                fn test_retry_config_no_retries() {
                    let config = RetryConfig::no_retries();
                    assert_eq!(config.max_retries, 0);
                }

                #[test]
                fn test_retry_config_aggressive() {
                    let config = RetryConfig::aggressive();
                    assert_eq!(config.max_retries, 5);
                    assert_eq!(config.initial_delay_secs, 30);
                }

                #[test]
                fn test_retry_config_conservative() {
                    let config = RetryConfig::conservative();
                    assert_eq!(config.max_retries, 2);
                    assert_eq!(config.initial_delay_secs, 120);
                }

                #[test]
                fn test_transient_error_detection_for_reqwest_send_failure() {
                    assert!(is_transient_error(
                        "LLM error: Request failed: error sending request for url (https://api.minimax.io/anthropic/v1/messages)"
                    ));
                }

                #[test]
                fn test_retry_state_new() {
                    let state = RetryState::new();
                    assert_eq!(state.attempt, 0);
                    assert!(state.last_error.is_none());
                    assert!(state.next_retry_at.is_none());
                }

                #[test]
                fn test_should_retry_transient_error() {
                    let config = RetryConfig::default();
                    let state = RetryState::new();

                    // Transient errors should be retried
                    assert!(state.should_retry(&config, "Connection timeout"));
                    assert!(state.should_retry(&config, "Rate limit exceeded"));
                    assert!(state.should_retry(&config, "503 Service Unavailable"));
                }

                #[test]
                fn test_should_not_retry_auth_error() {
                    let config = RetryConfig::default();
                    let state = RetryState::new();

                    // Auth errors should not be retried
                    assert!(!state.should_retry(&config, "401 Unauthorized"));
                    assert!(!state.should_retry(&config, "Invalid API key"));
                    assert!(!state.should_retry(&config, "403 Forbidden"));
                }

                #[test]
                fn test_should_not_retry_when_exhausted() {
                    let config = RetryConfig::default();
                    let mut state = RetryState::new();
                    state.attempt = config.max_retries;

                    // Even transient errors should not retry when exhausted
                    assert!(!state.should_retry(&config, "Connection timeout"));
                }

                #[test]
                fn test_calculate_delay_exponential() {
                    let config = RetryConfig {
                        max_retries: 5,
                        initial_delay_secs: 10,
                        max_delay_secs: 1000,
                        backoff_multiplier: 2.0,
                        jitter_enabled: false,
                        jitter_factor: 0.0,
                    };

                    let mut state = RetryState::new();

                    // First retry: 10 * 2^0 = 10
                    assert_eq!(state.calculate_delay(&config).as_secs(), 10);

                    state.attempt = 1;
                    // Second retry: 10 * 2^1 = 20
                    assert_eq!(state.calculate_delay(&config).as_secs(), 20);

                    state.attempt = 2;
                    // Third retry: 10 * 2^2 = 40
                    assert_eq!(state.calculate_delay(&config).as_secs(), 40);
                }

                #[test]
                fn test_calculate_delay_capped() {
                    let config = RetryConfig {
                        max_retries: 10,
                        initial_delay_secs: 100,
                        max_delay_secs: 500,
                        backoff_multiplier: 2.0,
                        jitter_enabled: false,
                        jitter_factor: 0.0,
                    };

                    let mut state = RetryState::new();
                    state.attempt = 5;

                    // Should be capped at max_delay_secs
                    assert_eq!(state.calculate_delay(&config).as_secs(), 500);
                }

                #[test]
                fn test_record_failure() {
                    let config = RetryConfig::default();
                    let mut state = RetryState::new();

                    state.record_failure("Connection timeout", &config);

                    assert_eq!(state.attempt, 1);
                    assert_eq!(state.total_failures, 1);
                    assert_eq!(state.last_error, Some("Connection timeout".to_string()));
                    assert!(state.next_retry_at.is_some());
                    assert!(state.last_failure_at.is_some());
                }

                #[test]
                fn test_record_failure_non_transient() {
                    let config = RetryConfig::default();
                    let mut state = RetryState::new();

                    state.record_failure("401 Unauthorized", &config);

                    assert_eq!(state.attempt, 1);
                    // Non-transient error: no next_retry_at
                    assert!(state.next_retry_at.is_none());
                }

                #[test]
                fn test_reset() {
                    let config = RetryConfig::default();
                    let mut state = RetryState::new();

                    state.record_failure("Connection timeout", &config);
                    state.record_failure("Connection timeout", &config);

                    state.reset();

                    assert_eq!(state.attempt, 0);
                    assert!(state.last_error.is_none());
                    assert!(state.next_retry_at.is_none());
                    // total_failures is preserved
                    assert_eq!(state.total_failures, 2);
                }

                #[test]
                fn test_is_exhausted() {
                    let config = RetryConfig::default();
                    let mut state = RetryState::new();

                    assert!(!state.is_exhausted(&config));

                    state.attempt = config.max_retries;
                    assert!(state.is_exhausted(&config));
                }

                #[test]
                fn test_is_transient_error() {
                    // Transient errors
                    assert!(is_transient_error("Connection timeout"));
                    assert!(is_transient_error("Rate limit exceeded"));
                    assert!(is_transient_error("503 Service Unavailable"));
                    assert!(is_transient_error("504 Gateway Timeout"));
                    assert!(is_transient_error("429 Too Many Requests"));
                    assert!(is_transient_error("Network error"));
                    assert!(is_transient_error("Connection reset by peer"));

                    // Non-transient errors
                    assert!(!is_transient_error("401 Unauthorized"));
                    assert!(!is_transient_error("Invalid API key"));
                    assert!(!is_transient_error("403 Forbidden"));
                    assert!(!is_transient_error("404 Not Found"));
                    assert!(!is_transient_error("400 Bad Request"));
                    assert!(!is_transient_error("Invalid model specified"));
                }

                #[test]
                fn test_error_category() {
                    assert_eq!(
                        ErrorCategory::from_error("Connection timeout"),
                        ErrorCategory::Transient
                    );
                    assert_eq!(
                        ErrorCategory::from_error("401 Unauthorized"),
                        ErrorCategory::AuthError
                    );
                    assert_eq!(
                        ErrorCategory::from_error("404 Not Found"),
                        ErrorCategory::NotFound
                    );
                    assert_eq!(
                        ErrorCategory::from_error("400 Bad Request"),
                        ErrorCategory::ClientError
                    );
                    assert_eq!(
                        ErrorCategory::from_error("Some unknown error"),
                        ErrorCategory::Unknown
                    );
                }

                #[test]
                fn test_error_category_should_retry() {
                    assert!(ErrorCategory::Transient.should_retry());
                    assert!(!ErrorCategory::AuthError.should_retry());
                    assert!(!ErrorCategory::ClientError.should_retry());
                    assert!(!ErrorCategory::NotFound.should_retry());
                    assert!(!ErrorCategory::Unknown.should_retry());
                }

                #[test]
                fn test_status_string() {
                    let config = RetryConfig::default();
                    let mut state = RetryState::new();

                    assert_eq!(state.status_string(&config), "Not retried");

                    state.record_failure("Connection timeout", &config);
                    assert!(state.status_string(&config).contains("Retry 2/3"));

                    state.attempt = config.max_retries;
                    assert!(state.status_string(&config).contains("Exhausted"));
                }
            }
        }

        pub use executor::{AgentRuntimeExecutor, SessionInputMode, SessionTurnRuntimeOptions};
        #[cfg(any(test, feature = "test-utils"))]
        pub use executor::{TestLlmFactoryGuard, install_test_llm_factory};
        pub use failover::{FailoverConfig, FailoverManager, ModelStatus, execute_with_failover};
        pub use retry::{ErrorCategory, RetryConfig, RetryState, is_transient_error};

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum ExecutionErrorKind {
            Authentication,
            RateLimited,
            Timeout,
            Tool,
            Model,
            Validation,
            Internal,
            UserInterrupted,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum RetryClass {
            Retryable,
            NonRetryable,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct ExecutionErrorClassification {
            pub kind: ExecutionErrorKind,
            pub retry_class: RetryClass,
        }

        impl ExecutionErrorClassification {
            pub const fn new(kind: ExecutionErrorKind, retry_class: RetryClass) -> Self {
                Self { kind, retry_class }
            }
        }

        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct CompactionMetrics {
            pub event_count: u32,
            pub tokens_before: usize,
            pub tokens_after: usize,
            pub messages_compacted: usize,
        }

        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct ExecutionMetrics {
            pub iterations: Option<u32>,
            pub active_model: Option<String>,
            pub final_model: Option<ModelId>,
            pub message_count: usize,
            pub compaction: Option<CompactionMetrics>,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct ExecutionFailure {
            pub message: String,
            pub classification: ExecutionErrorClassification,
            pub cause: Option<String>,
        }

        #[derive(Debug, Clone)]
        pub struct ExecutionOutcome {
            pub output: String,
            pub messages: Vec<Message>,
            pub success: bool,
            pub metrics: ExecutionMetrics,
            pub failure: Option<ExecutionFailure>,
        }

        impl ExecutionOutcome {
            pub fn success(output: String, messages: Vec<Message>) -> Self {
                let metrics = ExecutionMetrics {
                    message_count: messages.len(),
                    ..ExecutionMetrics::default()
                };
                Self {
                    output,
                    messages,
                    success: true,
                    metrics,
                    failure: None,
                }
            }

            pub fn success_with_compaction(
                output: String,
                messages: Vec<Message>,
                compaction: CompactionMetrics,
            ) -> Self {
                let message_count = messages.len();
                Self {
                    output,
                    messages,
                    success: true,
                    metrics: ExecutionMetrics {
                        message_count,
                        compaction: Some(compaction),
                        ..ExecutionMetrics::default()
                    },
                    failure: None,
                }
            }

            pub fn failure(
                message: impl Into<String>,
                classification: ExecutionErrorClassification,
                cause: Option<String>,
            ) -> Self {
                let message = message.into();
                Self {
                    output: message.clone(),
                    messages: Vec::new(),
                    success: false,
                    metrics: ExecutionMetrics::default(),
                    failure: Some(ExecutionFailure {
                        message,
                        classification,
                        cause,
                    }),
                }
            }

            pub fn with_metrics(mut self, metrics: ExecutionMetrics) -> Self {
                self.metrics = metrics;
                self
            }
        }

        #[derive(Debug, Clone)]
        pub struct SessionExecutionResult {
            pub output: String,
            pub iterations: u32,
            pub active_model: String,
            pub final_model: ModelId,
            pub metrics: ExecutionMetrics,
        }

        impl SessionExecutionResult {
            pub fn new(
                output: String,
                iterations: u32,
                active_model: String,
                final_model: ModelId,
            ) -> Self {
                Self {
                    output,
                    iterations,
                    active_model: active_model.clone(),
                    final_model,
                    metrics: ExecutionMetrics {
                        iterations: Some(iterations),
                        active_model: Some(active_model),
                        final_model: Some(final_model),
                        ..ExecutionMetrics::default()
                    },
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn success_outcome_sets_message_count() {
                let outcome = ExecutionOutcome::success("ok".to_string(), Vec::new());
                assert!(outcome.success);
                assert_eq!(outcome.metrics.message_count, 0);
                assert!(outcome.failure.is_none());
            }

            #[test]
            fn failure_outcome_captures_classification() {
                let outcome = ExecutionOutcome::failure(
                    "boom",
                    ExecutionErrorClassification::new(
                        ExecutionErrorKind::Internal,
                        RetryClass::NonRetryable,
                    ),
                    Some("panic".to_string()),
                );
                assert!(!outcome.success);
                let failure = outcome.failure.expect("failure");
                assert_eq!(failure.message, "boom");
                assert_eq!(failure.cause.as_deref(), Some("panic"));
                assert_eq!(failure.classification.kind, ExecutionErrorKind::Internal);
            }

            #[test]
            fn session_execution_result_populates_metrics() {
                let result = SessionExecutionResult::new(
                    "ok".to_string(),
                    3,
                    "gpt-5".to_string(),
                    ModelId::Gpt5,
                );
                assert_eq!(result.metrics.iterations, Some(3));
                assert_eq!(result.metrics.active_model.as_deref(), Some("gpt-5"));
                assert_eq!(result.metrics.final_model, Some(ModelId::Gpt5));
                assert_eq!(result.final_model, ModelId::Gpt5);
            }
        }
    }
    pub mod session_turn {
        mod turn_persistence {
            use types::MessageExecution;

            /// Build persisted turn payload (execution metadata + user input text).
            pub fn build_turn_persistence_payload(
                input: &str,
                duration_ms: u64,
                iterations: u32,
            ) -> (MessageExecution, String) {
                let execution = MessageExecution::new().complete(duration_ms, iterations);
                let persisted_input = input.to_string();
                (execution, persisted_input)
            }
        }
        pub use turn_persistence::build_turn_persistence_payload;
    }
    pub mod subagent {
        //! Storage-backed sub-agent definition adapters.
        //!
        //! This module is intentionally limited to definition lookup and registry
        //! plumbing. Runtime execution primitives such as `SubagentTracker`,
        //! `SubagentManagerImpl`, and `spawn_subagent` are owned by `agent`.

        pub mod definition {
            //! Agent type definitions for spawnable sub-agents.
            //!
            //! This module defines the available agent types that can be spawned
            //! by the main agent, including their capabilities and system prompts.

            use crate::{AgentStorage, StoredAgent};
            use ::agent::agent::{SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary};
            use parking_lot::RwLock;
            use serde::{Deserialize, Serialize};
            use specta::Type;
            use std::collections::HashMap;
            use std::sync::Arc;
            use std::time::{Duration, Instant};
            use tracing::warn;

            fn subagent_default_tool_names() -> Vec<String> {
                [
                    "bash",
                    "file",
                    "edit",
                    "multiedit",
                    "patch",
                    "glob",
                    "grep",
                    "load_skill",
                    "run_skill",
                ]
                .into_iter()
                .map(str::to_string)
                .collect()
            }

            /// Agent definition describing a spawnable agent type
            #[derive(Debug, Clone, Serialize, Deserialize, Type)]
            pub struct AgentDefinition {
                /// Unique identifier (e.g., "researcher", "coder")
                pub id: String,

                /// Display name
                pub name: String,

                /// Description of when to use this agent
                pub description: String,

                /// System prompt for the agent
                pub system_prompt: String,

                /// List of allowed tool names
                pub allowed_tools: Vec<String>,

                /// Optional specific model to use
                pub model: Option<String>,

                /// Maximum iterations for ReAct loop
                pub max_iterations: Option<u32>,

                /// Whether this agent can be spawned by other agents
                pub callable: bool,

                /// Tags for categorization
                pub tags: Vec<String>,
            }

            /// Registry of available agent definitions
            #[derive(Clone)]
            pub struct AgentDefinitionRegistry {
                definitions: HashMap<String, AgentDefinition>,
            }

            impl AgentDefinitionRegistry {
                /// Create a new empty registry
                pub fn new() -> Self {
                    Self {
                        definitions: HashMap::new(),
                    }
                }

                /// Create a registry with built-in agent definitions
                pub fn with_builtins() -> Self {
                    let mut registry = Self::new();
                    for def in builtin_agents() {
                        registry.register(def);
                    }
                    registry
                }

                /// Build a registry from persisted agents in storage.
                pub fn from_agents(agents: &[StoredAgent]) -> Self {
                    let mut registry = Self::new();
                    for stored in agents {
                        registry.register(Self::from_stored_agent(stored));
                    }
                    registry
                }

                /// Register an agent definition
                pub fn register(&mut self, definition: AgentDefinition) {
                    self.definitions.insert(definition.id.clone(), definition);
                }

                /// Get an agent definition by ID
                pub fn get(&self, id: &str) -> Option<&AgentDefinition> {
                    let query = id.trim();
                    if query.is_empty() {
                        return None;
                    }

                    if let Some(definition) = self.definitions.get(query) {
                        return Some(definition);
                    }

                    let prefix_matches: Vec<&AgentDefinition> = self
                        .definitions
                        .values()
                        .filter(|definition| definition.id.starts_with(query))
                        .collect();
                    if prefix_matches.len() == 1 {
                        return prefix_matches.first().copied();
                    }

                    let normalized_query = normalize_identifier(query);
                    if normalized_query.is_empty() {
                        return None;
                    }

                    let normalized_matches: Vec<&AgentDefinition> = self
                        .definitions
                        .values()
                        .filter(|definition| {
                            normalize_identifier(&definition.id) == normalized_query
                                || normalize_identifier(&definition.name) == normalized_query
                        })
                        .collect();
                    if normalized_matches.len() == 1 {
                        return normalized_matches.first().copied();
                    }

                    None
                }

                /// List all agent definitions
                pub fn list(&self) -> Vec<&AgentDefinition> {
                    self.definitions.values().collect()
                }

                /// List callable agent definitions
                pub fn callable(&self) -> Vec<&AgentDefinition> {
                    self.definitions.values().filter(|d| d.callable).collect()
                }

                /// Find agents by tag
                pub fn by_tag(&self, tag: &str) -> Vec<&AgentDefinition> {
                    self.definitions
                        .values()
                        .filter(|d| d.tags.contains(&tag.to_string()))
                        .collect()
                }

                fn from_stored_agent(stored: &StoredAgent) -> AgentDefinition {
                    let default_tools = subagent_default_tool_names();
                    let allowed_tools = stored
                        .agent
                        .tools
                        .clone()
                        .filter(|tools| !tools.is_empty())
                        .unwrap_or(default_tools);
                    let prompt = stored
                        .agent
                        .prompt
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| format!("You are {}.", stored.name));
                    let model = stored
                        .agent
                        .resolved_model_ref()
                        .map(|model_ref| model_ref.model.as_serialized_str().to_string());

                    AgentDefinition {
                        id: stored.id.clone(),
                        name: stored.name.clone(),
                        description: summarize_prompt(stored.agent.prompt.as_deref()),
                        system_prompt: prompt,
                        allowed_tools,
                        model,
                        max_iterations: None,
                        callable: true,
                        tags: vec!["stored".to_string()],
                    }
                }
            }

            impl Default for AgentDefinitionRegistry {
                fn default() -> Self {
                    Self::with_builtins()
                }
            }

            impl SubagentDefLookup for AgentDefinitionRegistry {
                fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                    self.get(id).map(|def| SubagentDefSnapshot {
                        name: def.name.clone(),
                        system_prompt: def.system_prompt.clone(),
                        allowed_tools: def.allowed_tools.clone(),
                        max_iterations: def.max_iterations,
                        default_model: def.model.clone(),
                    })
                }

                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    self.callable()
                        .into_iter()
                        .map(|def| SubagentDefSummary {
                            id: def.id.clone(),
                            name: def.name.clone(),
                            description: def.description.clone(),
                            tags: def.tags.clone(),
                        })
                        .collect()
                }
            }

            /// Dynamic sub-agent lookup backed by persisted agent storage.
            ///
            /// This keeps `spawn_subagent` definitions in sync with runtime agent CRUD
            /// without requiring daemon restart.
            #[derive(Clone)]
            pub struct StorageBackedSubagentLookup {
                agent_storage: AgentStorage,
                fallback: AgentDefinitionRegistry,
                cache_ttl: Duration,
                cache: Arc<RwLock<Option<CachedRegistry>>>,
            }

            #[derive(Clone)]
            struct CachedRegistry {
                loaded_at: Instant,
                registry: AgentDefinitionRegistry,
            }

            impl StorageBackedSubagentLookup {
                pub fn new(agent_storage: AgentStorage) -> Self {
                    Self {
                        agent_storage,
                        fallback: AgentDefinitionRegistry::with_builtins(),
                        cache_ttl: Duration::from_secs(5),
                        cache: Arc::new(RwLock::new(None)),
                    }
                }

                pub fn with_cache_ttl(mut self, cache_ttl: Duration) -> Self {
                    self.cache_ttl = cache_ttl;
                    self
                }

                fn load_registry(&self) -> Option<AgentDefinitionRegistry> {
                    if let Some(cached) = self
                        .cache
                        .read()
                        .as_ref()
                        .filter(|entry| entry.loaded_at.elapsed() <= self.cache_ttl)
                    {
                        return Some(cached.registry.clone());
                    }

                    match self.agent_storage.list_agents() {
                        Ok(agents) => {
                            let registry = AgentDefinitionRegistry::from_agents(&agents);
                            *self.cache.write() = Some(CachedRegistry {
                                loaded_at: Instant::now(),
                                registry: registry.clone(),
                            });
                            Some(registry)
                        }
                        Err(error) => {
                            warn!(error = %error, "Failed to load sub-agent definitions from storage");
                            self.cache
                                .read()
                                .as_ref()
                                .map(|entry| entry.registry.clone())
                        }
                    }
                }
            }

            impl SubagentDefLookup for StorageBackedSubagentLookup {
                fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                    if let Some(registry) = self.load_registry()
                        && let Some(snapshot) = registry.lookup(id)
                    {
                        return Some(snapshot);
                    }
                    self.fallback.lookup(id)
                }

                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    if let Some(registry) = self.load_registry() {
                        let callable = registry.list_callable();
                        if !callable.is_empty() {
                            return callable;
                        }
                    }
                    self.fallback.list_callable()
                }
            }

            fn summarize_prompt(prompt: Option<&str>) -> String {
                let Some(prompt) = prompt else {
                    return "Stored agent definition".to_string();
                };

                let first_line = prompt
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or_default()
                    .trim_start_matches('#')
                    .trim();
                if first_line.is_empty() {
                    return "Stored agent definition".to_string();
                }

                if first_line.chars().count() <= 120 {
                    first_line.to_string()
                } else {
                    format!("{}...", first_line.chars().take(120).collect::<String>())
                }
            }

            fn normalize_identifier(value: &str) -> String {
                let mut normalized = String::with_capacity(value.len());
                let mut previous_dash = false;

                for ch in value.trim().chars() {
                    if ch.is_ascii_alphanumeric() {
                        normalized.push(ch.to_ascii_lowercase());
                        previous_dash = false;
                        continue;
                    }

                    if !previous_dash {
                        normalized.push('-');
                        previous_dash = true;
                    }
                }

                normalized.trim_matches('-').to_string()
            }

            /// Built-in agent definitions.
            /// These are now minimal placeholders - actual prompts are loaded from ~/.restflow/agents/.
            /// The registry is populated from database records at runtime.
            pub fn builtin_agents() -> Vec<AgentDefinition> {
                vec![]
            }

            #[cfg(test)]
            mod tests {
                use super::{AgentDefinitionRegistry, builtin_agents};
                use crate::runtime::subagent::definition::StorageBackedSubagentLookup;
                use crate::test_support::RestflowTestEnv;
                use crate::{AgentStorage, StoredAgent};
                use ::agent::agent::SubagentDefLookup;
                use std::time::Duration;
                use types::{AgentNode, ModelId, ModelRef};

                fn stored_agent(
                    id: &str,
                    name: &str,
                    prompt: Option<&str>,
                    tools: Option<Vec<String>>,
                    model: Option<ModelId>,
                ) -> StoredAgent {
                    StoredAgent {
                        id: id.to_string(),
                        name: name.to_string(),
                        agent: AgentNode {
                            model_ref: model.map(ModelRef::from_model),
                            prompt: prompt.map(str::to_string),
                            tools,
                            ..Default::default()
                        },
                        prompt_file: None,
                        created_at: None,
                        updated_at: None,
                    }
                }

                #[test]
                fn test_builtin_agents_empty() {
                    // No built-in agents - they are loaded from ~/.restflow/agents/ at runtime
                    let agents = builtin_agents();
                    assert!(agents.is_empty());
                }

                #[test]
                fn test_registry_empty() {
                    let registry = AgentDefinitionRegistry::with_builtins();
                    // No built-in agents
                    assert!(registry.list().is_empty());
                    assert!(registry.callable().is_empty());
                }

                #[test]
                fn test_registry_by_tag_empty() {
                    let registry = AgentDefinitionRegistry::with_builtins();
                    let coding_agents = registry.by_tag("coding");
                    assert!(coding_agents.is_empty());
                }

                #[test]
                fn test_registry_from_agents_supports_id_and_name_lookup() {
                    let stored = stored_agent(
                        "agent-1",
                        "Research Coder",
                        Some("# Research specialist\nFocus on code and docs"),
                        Some(vec!["web_search".to_string(), "file".to_string()]),
                        Some(ModelId::MiniMaxM25CodingPlan),
                    );
                    let registry = AgentDefinitionRegistry::from_agents(&[stored]);

                    assert!(registry.get("agent-1").is_some());
                    assert!(registry.get("Research Coder").is_some());
                    assert!(registry.get("research-coder").is_some());

                    let snapshot = registry.lookup("research-coder").unwrap();
                    assert_eq!(
                        snapshot.default_model.as_deref(),
                        Some("minimax-coding-plan-m2-5")
                    );
                    assert!(snapshot.allowed_tools.contains(&"web_search".to_string()));
                }

                #[test]
                fn test_registry_from_agents_falls_back_to_default_tools() {
                    let stored =
                        stored_agent("agent-2", "No Tool Agent", Some("Prompt"), None, None);
                    let registry = AgentDefinitionRegistry::from_agents(&[stored]);
                    let snapshot = registry.lookup("agent-2").unwrap();
                    assert!(!snapshot.allowed_tools.is_empty());
                    assert!(snapshot.allowed_tools.contains(&"bash".to_string()));
                }

                #[test]
                fn test_name_lookup_returns_none_when_ambiguous() {
                    let agents = vec![
                        stored_agent("a-1", "Data Reviewer", Some("Prompt A"), None, None),
                        stored_agent("a-2", "data-reviewer", Some("Prompt B"), None, None),
                    ];
                    let registry = AgentDefinitionRegistry::from_agents(&agents);
                    assert!(registry.get("data-reviewer").is_none());
                }

                #[test]
                fn test_storage_backed_lookup_cache_holds_snapshot_until_ttl_expires() {
                    let env = RestflowTestEnv::new();

                    let storage =
                        AgentStorage::new_file_backed_path(env.root().join("agents")).unwrap();

                    let lookup = StorageBackedSubagentLookup::new(storage.clone())
                        .with_cache_ttl(Duration::from_secs(60));

                    assert!(lookup.lookup("cache-agent").is_none());
                    storage
                        .create_agent("Cache Agent".to_string(), AgentNode::new())
                        .unwrap();

                    // Cache should still serve the previous empty snapshot.
                    assert!(lookup.lookup("cache-agent").is_none());
                }

                #[test]
                fn test_storage_backed_lookup_refreshes_after_ttl() {
                    let env = RestflowTestEnv::new();

                    let storage =
                        AgentStorage::new_file_backed_path(env.root().join("agents")).unwrap();

                    let lookup = StorageBackedSubagentLookup::new(storage.clone())
                        .with_cache_ttl(Duration::from_millis(5));

                    assert!(lookup.lookup("refresh-agent").is_none());
                    storage
                        .create_agent("Refresh Agent".to_string(), AgentNode::new())
                        .unwrap();

                    std::thread::sleep(Duration::from_millis(20));
                    assert!(lookup.lookup("refresh-agent").is_some());
                }
            }
        }

        pub use definition::{
            AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
        };
    }

    // Public surface rule:
    // - `runner::runtime` re-exports durable runner-owned execution APIs.
    // - AI-owned subagent runtime state stays exported from `agent` /
    //   `types` so ownership remains unambiguous.
    pub use self::agent::build_agent_system_prompt;
    pub use self::agent::tools::{
        BashConfig, BashTool, FileConfig, FileTool, ListSubagentsTool, LoadSkillTool,
        SpawnSubagentTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult, WaitSubagentsTool,
        default_registry, effective_main_agent_tool_names, main_agent_default_tool_names,
        registry_from_allowlist, secret_resolver_from_storage,
    };
    pub use execution_context::{ExecutionContext, ExecutionRole};
    pub use orchestrator::AgentOrchestratorImpl;
    pub use session_runner::{AgentRuntimeExecutor, SessionExecutionResult, SessionInputMode};
    pub use subagent::{
        AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
    };
}

pub use runtime::*;
