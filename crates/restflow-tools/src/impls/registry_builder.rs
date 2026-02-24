//! Tool registry builder with configuration types.
//!
//! Provides BashConfig, FileConfig, and ToolRegistryBuilder for constructing
//! a ToolRegistry with commonly used tools.

use std::path::PathBuf;
use std::sync::Arc;

use crate::SecretResolver;
use crate::ToolRegistry;
use crate::impls::batch::BatchTool;
use crate::impls::list_agents::ListAgentsTool;
use crate::impls::monty_python::{PythonTool, RunPythonTool};
use crate::impls::spawn::SpawnTool;
use crate::impls::spawn_agent::SpawnAgentTool;
use crate::impls::transcribe::TranscribeTool;
use crate::impls::use_skill::UseSkillTool;
use crate::impls::vision::VisionTool;
use crate::impls::wait_agents::WaitAgentsTool;
use crate::impls::{BashTool, DiscordTool, EmailTool, FileTool, HttpTool, SlackTool, TelegramTool};
use crate::security::bash_security::BashSecurityConfig;
use restflow_traits::skill::SkillProvider;
use restflow_traits::{SubagentManager, SubagentSpawner};

// Web tools
use crate::impls::jina_reader::JinaReaderTool;
use crate::impls::web_fetch::WebFetchTool;
use crate::impls::web_search::WebSearchTool;

// Storage-backed tools
use crate::impls::agent_crud::AgentCrudTool;
use crate::impls::auth_profile::AuthProfileTool;
use crate::impls::background_agent::BackgroundAgentTool;
use crate::impls::config::ConfigTool;
use crate::impls::diagnostics::DiagnosticsTool;
use crate::impls::edit::EditTool;
use crate::impls::file_tracker::FileTracker;
use crate::impls::glob_tool::GlobTool;
use crate::impls::grep_tool::GrepTool;
use crate::impls::kv_store::KvStoreTool;
use crate::impls::manage_ops::ManageOpsTool;
use crate::impls::marketplace::MarketplaceTool;
use crate::impls::memory_mgmt::MemoryManagementTool;
use crate::impls::memory_store::{
    DeleteMemoryTool, ListMemoryTool, ReadMemoryTool, SaveMemoryTool,
};
use crate::impls::multiedit::MultiEditTool;
use crate::impls::patch::PatchTool;
use crate::impls::save_deliverable::SaveDeliverableTool;
use crate::impls::secrets::SecretsTool;
use crate::impls::security_query::SecurityQueryTool;
use crate::impls::session::SessionTool;
use crate::impls::skill::SkillTool;
use crate::impls::task_list::TaskListTool;
use crate::impls::terminal::TerminalTool;
use crate::impls::trigger::TriggerTool;
use crate::impls::unified_memory_search::UnifiedMemorySearchTool;
use crate::impls::work_item::WorkItemTool;

// Store traits
use restflow_traits::store::{
    AgentStore, AuthProfileStore, BackgroundAgentStore, DeliverableStore, DiagnosticsProvider,
    KvStore, MarketplaceStore, MemoryManager, MemoryStore, OpsProvider, SecurityQueryProvider,
    SessionStore, TerminalStore, TriggerStore, UnifiedMemorySearch, WorkItemProvider,
};

// Concrete storage types
use restflow_storage::{ConfigStorage, SecretStorage};

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
    /// Convert into a [`FileTool`] with a new internal tracker.
    pub fn into_file_tool(self) -> FileTool {
        let mut tool = FileTool::new().with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        }
        tool
    }

    /// Convert into a [`FileTool`] using a shared [`FileTracker`].
    pub fn into_file_tool_with_tracker(self, tracker: Arc<FileTracker>) -> FileTool {
        let mut tool = FileTool::with_tracker(tracker).with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        }
        tool
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

    pub fn with_bash(mut self, config: BashConfig) -> Self {
        self.registry.register(config.into_bash_tool());
        self
    }

    pub fn with_file(mut self, config: FileConfig) -> Self {
        self.registry
            .register(config.into_file_tool_with_tracker(self.tracker.clone()));
        self
    }

    pub fn with_http(mut self) -> Result<Self, reqwest::Error> {
        self.registry.register(HttpTool::new()?);
        Ok(self)
    }

    pub fn with_email(mut self) -> Self {
        self.registry.register(EmailTool::new());
        self
    }

    pub fn with_telegram(mut self) -> Result<Self, reqwest::Error> {
        self.registry.register(TelegramTool::new()?);
        Ok(self)
    }

    pub fn with_discord(mut self) -> Result<Self, reqwest::Error> {
        self.registry.register(DiscordTool::new()?);
        Ok(self)
    }

    pub fn with_slack(mut self) -> Result<Self, reqwest::Error> {
        self.registry.register(SlackTool::new()?);
        Ok(self)
    }

    pub fn with_python(mut self) -> Self {
        self.registry.register(RunPythonTool::new());
        self.registry.register(PythonTool::new());
        self
    }

    pub fn with_transcribe(mut self, resolver: SecretResolver) -> Result<Self, reqwest::Error> {
        self.registry.register(TranscribeTool::new(resolver)?);
        Ok(self)
    }

    pub fn with_vision(mut self, resolver: SecretResolver) -> Result<Self, reqwest::Error> {
        self.registry.register(VisionTool::new(resolver)?);
        Ok(self)
    }

    pub fn with_spawn(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.registry.register(SpawnTool::new(spawner));
        self
    }

    pub fn with_spawn_agent(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(SpawnAgentTool::new(manager));
        self
    }

    pub fn with_wait_agents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(WaitAgentsTool::new(manager));
        self
    }

    pub fn with_list_agents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(ListAgentsTool::new(manager));
        self
    }

    pub fn with_use_skill(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(UseSkillTool::new(provider));
        self
    }

    // --- Web tools ---

    pub fn with_web_fetch(mut self) -> Self {
        self.registry.register(WebFetchTool::new());
        self
    }

    pub fn with_jina_reader(mut self) -> Result<Self, reqwest::Error> {
        self.registry.register(JinaReaderTool::new()?);
        Ok(self)
    }

    pub fn with_web_search(mut self) -> Result<Self, reqwest::Error> {
        self.registry.register(WebSearchTool::new()?);
        Ok(self)
    }

    pub fn with_web_search_with_resolver(
        mut self,
        resolver: SecretResolver,
    ) -> Result<Self, reqwest::Error> {
        self.registry
            .register(WebSearchTool::new()?.with_secret_resolver(resolver));
        Ok(self)
    }

    // --- Storage-backed tools ---

    pub fn with_diagnostics(mut self, provider: Arc<dyn DiagnosticsProvider>) -> Self {
        self.registry.register(DiagnosticsTool::new(provider));
        self
    }

    pub fn with_skill_tool(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(SkillTool::new(provider));
        self
    }

    pub fn with_session(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.registry
            .register(SessionTool::new(store).with_write(true));
        self
    }

    pub fn with_memory_management(mut self, manager: Arc<dyn MemoryManager>) -> Self {
        self.registry
            .register(MemoryManagementTool::new(manager).with_write(true));
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.registry.register(SaveMemoryTool::new(store.clone()));
        self.registry.register(ReadMemoryTool::new(store.clone()));
        self.registry.register(ListMemoryTool::new(store.clone()));
        self.registry.register(DeleteMemoryTool::new(store));
        self
    }

    pub fn with_deliverable(mut self, store: Arc<dyn DeliverableStore>) -> Self {
        self.registry.register(SaveDeliverableTool::new(store));
        self
    }

    pub fn with_unified_search(mut self, search: Arc<dyn UnifiedMemorySearch>) -> Self {
        self.registry.register(UnifiedMemorySearchTool::new(search));
        self
    }

    pub fn with_ops(mut self, provider: Arc<dyn OpsProvider>) -> Self {
        self.registry.register(ManageOpsTool::new(provider));
        self
    }

    pub fn with_kv_store(mut self, store: Arc<dyn KvStore>) -> Self {
        self.registry.register(KvStoreTool::new(store, None));
        self
    }

    pub fn with_work_items(mut self, provider: Arc<dyn WorkItemProvider>) -> Self {
        self.registry
            .register(WorkItemTool::new(provider).with_write(true));
        self
    }

    pub fn with_auth_profile(mut self, store: Arc<dyn AuthProfileStore>) -> Self {
        self.registry
            .register(AuthProfileTool::new(store).with_write(true));
        self
    }

    pub fn with_secrets(mut self, storage: Arc<SecretStorage>) -> Self {
        self.registry.register(SecretsTool::new(storage));
        self
    }

    pub fn with_config(mut self, storage: Arc<ConfigStorage>) -> Self {
        self.registry.register(ConfigTool::new(storage));
        self
    }

    pub fn with_agent_crud(mut self, store: Arc<dyn AgentStore>) -> Self {
        self.registry
            .register(AgentCrudTool::new(store).with_write(true));
        self
    }

    pub fn with_background_agent(mut self, store: Arc<dyn BackgroundAgentStore>) -> Self {
        self.registry
            .register(BackgroundAgentTool::new(store).with_write(true));
        self
    }

    pub fn with_marketplace(mut self, store: Arc<dyn MarketplaceStore>) -> Self {
        self.registry.register(MarketplaceTool::new(store));
        self
    }

    pub fn with_trigger(mut self, store: Arc<dyn TriggerStore>) -> Self {
        self.registry.register(TriggerTool::new(store));
        self
    }

    pub fn with_terminal(mut self, store: Arc<dyn TerminalStore>) -> Self {
        self.registry.register(TerminalTool::new(store));
        self
    }

    pub fn with_security_query(mut self, provider: Arc<dyn SecurityQueryProvider>) -> Self {
        self.registry.register(SecurityQueryTool::new(provider));
        self
    }

    pub fn with_patch(mut self) -> Self {
        self.registry.register(PatchTool::new(self.tracker.clone()));
        self
    }

    pub fn with_edit(self) -> Self {
        self.with_edit_and_diagnostics(None)
    }

    pub fn with_edit_and_diagnostics(
        mut self,
        diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    ) -> Self {
        let mut tool = EditTool::with_tracker(self.tracker.clone());
        if let Some(diag) = diagnostics {
            tool = tool.with_diagnostics_provider(diag);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_multiedit(self) -> Self {
        self.with_multiedit_and_diagnostics(None)
    }

    pub fn with_multiedit_and_diagnostics(
        mut self,
        diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    ) -> Self {
        let mut tool = MultiEditTool::with_tracker(self.tracker.clone());
        if let Some(diag) = diagnostics {
            tool = tool.with_diagnostics_provider(diag);
        }
        self.registry.register(tool);
        self
    }

    // --- Search tools ---

    pub fn with_glob(mut self) -> Self {
        self.registry.register(GlobTool::new());
        self
    }

    pub fn with_grep(mut self) -> Self {
        self.registry.register(GrepTool::new());
        self
    }

    pub fn with_task_list(mut self, provider: Arc<dyn WorkItemProvider>) -> Self {
        self.registry.register(TaskListTool::new(provider));
        self
    }

    /// Register the batch tool. This requires an `Arc<ToolRegistry>` containing
    /// the tools the batch tool can invoke. Typically used in a two-phase build:
    /// 1. Build the base registry with `build()` and wrap in `Arc`
    /// 2. Register the batch tool on the Arc'd registry
    ///
    /// Alternatively, use `build_with_batch()` which handles this automatically.
    pub fn with_batch(mut self, tools: Arc<ToolRegistry>) -> Self {
        self.registry.register(BatchTool::new(tools));
        self
    }

    pub fn build(self) -> ToolRegistry {
        self.registry
    }
}

/// Create a registry with default tools.
pub fn default_registry() -> Result<ToolRegistry, reqwest::Error> {
    Ok(ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .with_http()?
        .with_email()
        .with_telegram()?
        .with_discord()?
        .with_slack()?
        .with_python()
        .build())
}
