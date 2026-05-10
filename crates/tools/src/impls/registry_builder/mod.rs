//! Tool registry builder with configuration types.
//!
//! Provides BashConfig, FileConfig, and ToolRegistryBuilder for constructing
//! a ToolRegistry with commonly used tools.

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use crate::impls::agent_crud::AgentCrudTool;
use crate::impls::batch::BatchTool;
use crate::impls::config::ConfigTool;
use crate::impls::edit::EditTool;
use crate::impls::file_tracker::FileTracker;
use crate::impls::glob_tool::GlobTool;
use crate::impls::grep_tool::GrepTool;
use crate::impls::list_subagents::ListSubagentsTool;
use crate::impls::load_skill::LoadSkillTool;
use crate::impls::manage_ops::ManageOpsTool;
use crate::impls::multiedit::MultiEditTool;
use crate::impls::patch::PatchTool;
use crate::impls::secrets::{SecretGetPolicy, SecretsTool};
use crate::impls::session::SessionTool;
use crate::impls::skill::SkillTool;
use crate::impls::spawn_subagent::SpawnSubagentTool;
use crate::impls::wait_subagents::WaitSubagentsTool;
use crate::impls::{BashTool, FileTool};
use crate::{BashSecurityConfig, SecurityGate, ToolRegistry};
use types::skill::SkillProvider;
use types::store::{AgentStore, ConfigStore, OpsProvider, SecretStore, SessionStore};
use types::{AgentOperationAssessor, SubagentManager};

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
            allowed_paths: Vec::new(),
            allow_write: true,
            max_read_bytes: 1_000_000,
        }
    }
}

impl FileConfig {
    pub fn for_workspace_root(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            allowed_paths: vec![workspace_root.into()],
            ..Self::default()
        }
    }

    /// Convert into a [`FileTool`] with a new internal tracker.
    pub fn into_file_tool(self) -> FileTool {
        let require_base_dir = self.allowed_paths.is_empty();
        let mut tool = FileTool::new().with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        } else if require_base_dir {
            tool = tool.require_base_dir();
        }
        tool
    }

    /// Convert into a [`FileTool`] using a shared [`FileTracker`].
    pub fn into_file_tool_with_tracker(self, tracker: Arc<FileTracker>) -> FileTool {
        let require_base_dir = self.allowed_paths.is_empty();
        let mut tool = FileTool::with_tracker(tracker).with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        } else if require_base_dir {
            tool = tool.require_base_dir();
        }
        tool
    }
}

/// Configuration for manage_secrets tool behavior.
#[derive(Debug, Clone, Copy)]
pub struct SecretsConfig {
    /// Whether write operations are allowed.
    pub allow_write: bool,
    /// Policy for the `get` operation response payload.
    pub get_policy: SecretGetPolicy,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            allow_write: false,
            get_policy: SecretGetPolicy::Open,
        }
    }
}

/// Builder for creating a fully configured ToolRegistry.
pub struct ToolRegistryBuilder {
    pub registry: ToolRegistry,
    tracker: Arc<FileTracker>,
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
            tracker: Arc::new(FileTracker::new()),
        }
    }

    /// Get shared file tracker for external use.
    pub fn tracker(&self) -> Arc<FileTracker> {
        self.tracker.clone()
    }

    pub fn build(self) -> ToolRegistry {
        self.registry
    }

    /// Build the registry and automatically register the `batch` tool.
    ///
    /// This is a convenience for the two-phase setup required by `BatchTool`,
    /// which needs an `Arc<ToolRegistry>` containing the base tools it can call.
    pub fn build_with_batch(self) -> ToolRegistry {
        let mut registry = self.build();
        if registry.has("batch") {
            return registry;
        }

        let registry_arc = Arc::new(std::mem::take(&mut registry));
        for name in registry_arc.list() {
            if let Some(tool) = registry_arc.get(name) {
                registry.register_arc(tool);
            }
        }
        registry.register(BatchTool::new(registry_arc));
        registry
    }
}

/// Create a registry with default core tools.
pub fn default_registry() -> anyhow::Result<ToolRegistry> {
    Ok(ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .build())
}

impl ToolRegistryBuilder {
    pub fn with_bash(mut self, config: BashConfig) -> Self {
        self.registry.register(config.into_bash_tool());
        self
    }

    pub fn with_file(mut self, config: FileConfig) -> Self {
        self.registry
            .register(config.into_file_tool_with_tracker(self.tracker.clone()));
        self
    }

    pub fn with_patch(mut self) -> Self {
        self.registry.register(PatchTool::new(self.tracker.clone()));
        self
    }

    pub fn with_patch_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = PatchTool::new(self.tracker.clone()).require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_edit(mut self) -> Self {
        let tool = EditTool::with_tracker(self.tracker.clone());
        self.registry.register(tool);
        self
    }

    pub fn with_edit_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = EditTool::with_tracker(self.tracker.clone()).require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_multiedit(mut self) -> Self {
        let tool = MultiEditTool::with_tracker(self.tracker.clone());
        self.registry.register(tool);
        self
    }

    pub fn with_multiedit_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = MultiEditTool::with_tracker(self.tracker.clone()).require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_glob(mut self) -> Self {
        self.registry.register(GlobTool::new());
        self
    }

    pub fn with_glob_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = GlobTool::new().require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_grep(mut self) -> Self {
        self.registry.register(GrepTool::new());
        self
    }

    pub fn with_grep_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = GrepTool::new().require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    /// Register the batch tool. This requires an `Arc<ToolRegistry>` containing
    /// the tools the batch tool can invoke.
    pub fn with_batch(mut self, tools: Arc<ToolRegistry>) -> Self {
        self.registry.register(BatchTool::new(tools));
        self
    }

    pub fn with_spawn_subagent(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(SpawnSubagentTool::new(manager));
        self
    }

    pub fn with_wait_subagents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(WaitSubagentsTool::new(manager));
        self
    }

    pub fn with_list_subagents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(ListSubagentsTool::new(manager));
        self
    }

    pub fn with_load_skill(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(LoadSkillTool::new(provider));
        self
    }

    pub fn with_load_skill_with_security(
        mut self,
        provider: Arc<dyn SkillProvider>,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.registry
            .register(LoadSkillTool::new(provider).with_security(security_gate, agent_id, task_id));
        self
    }

    pub fn with_skill_tool(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(SkillTool::new(provider));
        self
    }

    pub fn with_skill_tool_with_security(
        mut self,
        provider: Arc<dyn SkillProvider>,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.registry
            .register(SkillTool::new(provider).with_security(security_gate, agent_id, task_id));
        self
    }

    pub fn with_session(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.registry
            .register(SessionTool::new(store).with_write(true));
        self
    }

    pub fn with_ops(mut self, provider: Arc<dyn OpsProvider>) -> Self {
        self.registry.register(ManageOpsTool::new(provider));
        self
    }

    pub fn with_secrets(mut self, store: Arc<dyn SecretStore>) -> Self {
        self = self.with_secrets_config(store, SecretsConfig::default());
        self
    }

    pub fn with_secrets_config(
        mut self,
        store: Arc<dyn SecretStore>,
        config: SecretsConfig,
    ) -> Self {
        self.registry.register(
            SecretsTool::new(store)
                .with_write(config.allow_write)
                .with_get_policy(config.get_policy),
        );
        self
    }

    pub fn with_config(mut self, store: Arc<dyn ConfigStore>) -> Self {
        self.registry.register(ConfigTool::new(store));
        self
    }

    pub fn with_agent_crud(mut self, store: Arc<dyn AgentStore>) -> Self {
        self.registry
            .register(AgentCrudTool::new(store).with_write(true));
        self
    }

    pub fn with_agent_crud_and_assessor(
        mut self,
        store: Arc<dyn AgentStore>,
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> Self {
        self.registry.register(
            AgentCrudTool::new(store)
                .with_assessor(assessor)
                .with_write(true),
        );
        self
    }
}
