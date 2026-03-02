//! System configuration storage.

use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("system_config");

const GLOBAL_CONFIG_ENV: &str = "RESTFLOW_GLOBAL_CONFIG";
const WORKSPACE_CONFIG_ENV: &str = "RESTFLOW_WORKSPACE_CONFIG";
const CONFIG_SUBDIR: &str = ".restflow";
const CONFIG_FILE_NAME: &str = "config.toml";

// Default configuration constants
const DEFAULT_WORKER_COUNT: usize = 4;
const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 1800; // 30 minutes
const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 600; // 10 minutes
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_CHAT_SESSION_RETENTION_DAYS: u32 = 30;
const DEFAULT_BACKGROUND_TASK_RETENTION_DAYS: u32 = 7;
const DEFAULT_CHECKPOINT_RETENTION_DAYS: u32 = 3;
const DEFAULT_MEMORY_CHUNK_RETENTION_DAYS: u32 = 90;
const DEFAULT_LOG_FILE_RETENTION_DAYS: u32 = 30;
const DEFAULT_MEMORY_SEARCH_LIMIT: u32 = 10;
const DEFAULT_SESSION_LIST_LIMIT: u32 = 20;
const DEFAULT_BG_PROGRESS_EVENT_LIMIT: usize = 10;
const DEFAULT_BG_MESSAGE_LIST_LIMIT: usize = 50;
const DEFAULT_BG_TRACE_LIST_LIMIT: usize = 50;
const DEFAULT_BG_TRACE_LINE_LIMIT: usize = 200;
const DEFAULT_BROWSER_TIMEOUT_SECS: u64 = 120;
const DEFAULT_PROCESS_SESSION_TTL_SECS: u64 = 30 * 60;
const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 300;
const MIN_RETENTION_DAYS: u32 = 1;
const MIN_WORKER_COUNT: usize = 1;
const MIN_TIMEOUT_SECONDS: u64 = 10;

/// Agent execution defaults (configurable at runtime via `manage_config`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentDefaults {
    /// Timeout for a single tool execution in seconds.
    pub tool_timeout_secs: u64,
    /// Default timeout for bash command execution in seconds.
    pub bash_timeout_secs: u64,
    /// Default timeout for Python code execution in seconds.
    pub python_timeout_secs: u64,
    /// Default timeout for browser tool execution in seconds.
    pub browser_timeout_secs: u64,
    /// TTL for finished process sessions in seconds.
    pub process_session_ttl_secs: u64,
    /// Default approval timeout for security checks in seconds.
    pub approval_timeout_secs: u64,
    /// Maximum ReAct loop iterations per agent run.
    pub max_iterations: usize,
    /// Default timeout for sub-agent execution in seconds.
    pub subagent_timeout_secs: u64,
    /// Maximum number of sub-agents that can run in parallel.
    pub max_parallel_subagents: usize,
    /// Maximum tool calls allowed per agent run.
    pub max_tool_calls: usize,
    /// Maximum wall-clock time per agent run in seconds.
    ///
    /// `None` disables wall-clock timeout for foreground agent runs.
    pub max_wall_clock_secs: Option<u64>,
    /// Default timeout for background agent task execution in seconds.
    pub default_task_timeout_secs: u64,
    /// Default max duration for background agent resource limits in seconds.
    pub default_max_duration_secs: u64,
    /// Fallback models for cross-provider failover (manually configured).
    /// Only used when primary model fails - does not auto-discover providers.
    /// Format: model names as strings (e.g., ["glm-4.7", "claude-sonnet-4-5"])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_models: Option<Vec<String>>,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            tool_timeout_secs: 300,
            bash_timeout_secs: 300,
            python_timeout_secs: 120,
            browser_timeout_secs: DEFAULT_BROWSER_TIMEOUT_SECS,
            process_session_ttl_secs: DEFAULT_PROCESS_SESSION_TTL_SECS,
            approval_timeout_secs: DEFAULT_APPROVAL_TIMEOUT_SECS,
            max_iterations: 100,
            subagent_timeout_secs: 3600,
            max_parallel_subagents: 200,
            max_tool_calls: 200,
            max_wall_clock_secs: None,
            default_task_timeout_secs: 1800,
            default_max_duration_secs: 1800,
            fallback_models: None,
        }
    }
}

impl AgentDefaults {
    fn validate(&self) -> Result<()> {
        if self.tool_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.tool_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.bash_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.bash_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.python_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.python_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.browser_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.browser_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.process_session_ttl_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.process_session_ttl_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.approval_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.approval_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.max_iterations == 0 {
            return Err(anyhow::anyhow!("agent.max_iterations must be at least 1"));
        }
        if self.subagent_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.subagent_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.max_parallel_subagents == 0 {
            return Err(anyhow::anyhow!(
                "agent.max_parallel_subagents must be at least 1"
            ));
        }
        if self.max_tool_calls == 0 {
            return Err(anyhow::anyhow!("agent.max_tool_calls must be at least 1"));
        }
        if let Some(timeout_secs) = self.max_wall_clock_secs
            && timeout_secs < MIN_TIMEOUT_SECONDS
        {
            return Err(anyhow::anyhow!(
                "agent.max_wall_clock_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.default_task_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.default_task_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.default_max_duration_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "agent.default_max_duration_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        Ok(())
    }
}

/// API-facing default limits used by MCP and adapter query operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiDefaults {
    /// Default `memory_search` result limit.
    pub memory_search_limit: u32,
    /// Default `chat_session_list` result limit.
    pub session_list_limit: u32,
    /// Default event limit for background progress queries.
    pub background_progress_event_limit: usize,
    /// Default message list limit for background agents.
    pub background_message_list_limit: usize,
    /// Default trace list limit for background agents.
    pub background_trace_list_limit: usize,
    /// Default trailing line limit when reading trace output.
    pub background_trace_line_limit: usize,
}

impl Default for ApiDefaults {
    fn default() -> Self {
        Self {
            memory_search_limit: DEFAULT_MEMORY_SEARCH_LIMIT,
            session_list_limit: DEFAULT_SESSION_LIST_LIMIT,
            background_progress_event_limit: DEFAULT_BG_PROGRESS_EVENT_LIMIT,
            background_message_list_limit: DEFAULT_BG_MESSAGE_LIST_LIMIT,
            background_trace_list_limit: DEFAULT_BG_TRACE_LIST_LIMIT,
            background_trace_line_limit: DEFAULT_BG_TRACE_LINE_LIMIT,
        }
    }
}

impl ApiDefaults {
    fn validate(&self) -> Result<()> {
        if self.memory_search_limit == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.memory_search_limit must be at least 1"
            ));
        }
        if self.session_list_limit == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.session_list_limit must be at least 1"
            ));
        }
        if self.background_progress_event_limit == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.background_progress_event_limit must be at least 1"
            ));
        }
        if self.background_message_list_limit == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.background_message_list_limit must be at least 1"
            ));
        }
        if self.background_trace_list_limit == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.background_trace_list_limit must be at least 1"
            ));
        }
        if self.background_trace_line_limit == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.background_trace_line_limit must be at least 1"
            ));
        }
        Ok(())
    }
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemConfig {
    pub worker_count: usize,
    pub task_timeout_seconds: u64,
    pub stall_timeout_seconds: u64,
    /// Default timeout for background API tasks in seconds.
    ///
    /// `None` disables timeout by default. Individual tasks may still configure
    /// their own `timeout_secs` and resource limits.
    #[serde(default)]
    pub background_api_timeout_seconds: Option<u64>,
    /// Timeout for interactive channel chat responses in seconds.
    ///
    /// `None` disables timeout for chat dispatching.
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub background_task_retention_days: u32,
    pub checkpoint_retention_days: u32,
    pub memory_chunk_retention_days: u32,
    /// Retention period for daemon and event log files on disk.
    /// 0 = keep forever, otherwise delete files older than N days.
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
    /// Agent execution defaults.
    #[serde(default)]
    pub agent: AgentDefaults,
    /// API operation default limits.
    #[serde(default)]
    pub api_defaults: ApiDefaults,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            worker_count: DEFAULT_WORKER_COUNT,
            task_timeout_seconds: DEFAULT_TASK_TIMEOUT_SECONDS,
            stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
            background_api_timeout_seconds: None,
            chat_response_timeout_seconds: None,
            max_retries: DEFAULT_MAX_RETRIES,
            chat_session_retention_days: DEFAULT_CHAT_SESSION_RETENTION_DAYS,
            background_task_retention_days: DEFAULT_BACKGROUND_TASK_RETENTION_DAYS,
            checkpoint_retention_days: DEFAULT_CHECKPOINT_RETENTION_DAYS,
            memory_chunk_retention_days: DEFAULT_MEMORY_CHUNK_RETENTION_DAYS,
            log_file_retention_days: DEFAULT_LOG_FILE_RETENTION_DAYS,
            experimental_features: Vec::new(),
            agent: AgentDefaults::default(),
            api_defaults: ApiDefaults::default(),
        }
    }
}

impl SystemConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.worker_count < MIN_WORKER_COUNT {
            return Err(anyhow::anyhow!(
                "Worker count must be at least {}",
                MIN_WORKER_COUNT
            ));
        }

        if self.task_timeout_seconds < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "Task timeout must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }

        if self.stall_timeout_seconds < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "Stall timeout must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }

        if let Some(timeout_secs) = self.background_api_timeout_seconds
            && timeout_secs < MIN_TIMEOUT_SECONDS
        {
            return Err(anyhow::anyhow!(
                "Background API timeout must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }

        if let Some(timeout_secs) = self.chat_response_timeout_seconds
            && timeout_secs < MIN_TIMEOUT_SECONDS
        {
            return Err(anyhow::anyhow!(
                "Chat response timeout must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }

        if self.max_retries == 0 {
            return Err(anyhow::anyhow!("Max retries must be at least 1"));
        }

        if self.chat_session_retention_days != 0
            && self.chat_session_retention_days < MIN_RETENTION_DAYS
        {
            return Err(anyhow::anyhow!(
                "Chat session retention must be 0 (forever) or at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.background_task_retention_days < MIN_RETENTION_DAYS {
            return Err(anyhow::anyhow!(
                "Background task retention must be at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.checkpoint_retention_days < MIN_RETENTION_DAYS {
            return Err(anyhow::anyhow!(
                "Checkpoint retention must be at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.memory_chunk_retention_days != 0
            && self.memory_chunk_retention_days < MIN_RETENTION_DAYS
        {
            return Err(anyhow::anyhow!(
                "Memory chunk retention must be 0 (forever) or at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        if self.log_file_retention_days != 0 && self.log_file_retention_days < MIN_RETENTION_DAYS {
            return Err(anyhow::anyhow!(
                "Log file retention must be 0 (forever) or at least {} day",
                MIN_RETENTION_DAYS
            ));
        }

        let mut seen = HashSet::new();
        for feature in &self.experimental_features {
            let normalized = feature.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return Err(anyhow::anyhow!(
                    "Experimental feature names must be non-empty strings"
                ));
            }
            if !seen.insert(normalized.clone()) {
                return Err(anyhow::anyhow!(
                    "Duplicate experimental feature: {}",
                    normalized
                ));
            }
        }

        self.agent.validate()?;
        self.api_defaults.validate()?;

        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AgentDefaultsOverride {
    pub tool_timeout_secs: Option<u64>,
    pub bash_timeout_secs: Option<u64>,
    pub python_timeout_secs: Option<u64>,
    pub browser_timeout_secs: Option<u64>,
    pub process_session_ttl_secs: Option<u64>,
    pub approval_timeout_secs: Option<u64>,
    pub max_iterations: Option<usize>,
    pub subagent_timeout_secs: Option<u64>,
    pub max_parallel_subagents: Option<usize>,
    pub max_tool_calls: Option<usize>,
    pub max_wall_clock_secs: Option<Option<u64>>,
    pub default_task_timeout_secs: Option<u64>,
    pub default_max_duration_secs: Option<u64>,
    pub fallback_models: Option<Option<Vec<String>>>,
}

impl AgentDefaultsOverride {
    fn apply_to(&self, agent: &mut AgentDefaults) {
        if let Some(value) = self.tool_timeout_secs {
            agent.tool_timeout_secs = value;
        }
        if let Some(value) = self.bash_timeout_secs {
            agent.bash_timeout_secs = value;
        }
        if let Some(value) = self.python_timeout_secs {
            agent.python_timeout_secs = value;
        }
        if let Some(value) = self.browser_timeout_secs {
            agent.browser_timeout_secs = value;
        }
        if let Some(value) = self.process_session_ttl_secs {
            agent.process_session_ttl_secs = value;
        }
        if let Some(value) = self.approval_timeout_secs {
            agent.approval_timeout_secs = value;
        }
        if let Some(value) = self.max_iterations {
            agent.max_iterations = value;
        }
        if let Some(value) = self.subagent_timeout_secs {
            agent.subagent_timeout_secs = value;
        }
        if let Some(value) = self.max_parallel_subagents {
            agent.max_parallel_subagents = value;
        }
        if let Some(value) = self.max_tool_calls {
            agent.max_tool_calls = value;
        }
        if let Some(value) = self.max_wall_clock_secs {
            agent.max_wall_clock_secs = value;
        }
        if let Some(value) = self.default_task_timeout_secs {
            agent.default_task_timeout_secs = value;
        }
        if let Some(value) = self.default_max_duration_secs {
            agent.default_max_duration_secs = value;
        }
        if let Some(value) = self.fallback_models.clone() {
            agent.fallback_models = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApiDefaultsOverride {
    pub memory_search_limit: Option<u32>,
    pub session_list_limit: Option<u32>,
    pub background_progress_event_limit: Option<usize>,
    pub background_message_list_limit: Option<usize>,
    pub background_trace_list_limit: Option<usize>,
    pub background_trace_line_limit: Option<usize>,
}

impl ApiDefaultsOverride {
    fn apply_to(&self, api_defaults: &mut ApiDefaults) {
        if let Some(value) = self.memory_search_limit {
            api_defaults.memory_search_limit = value;
        }
        if let Some(value) = self.session_list_limit {
            api_defaults.session_list_limit = value;
        }
        if let Some(value) = self.background_progress_event_limit {
            api_defaults.background_progress_event_limit = value;
        }
        if let Some(value) = self.background_message_list_limit {
            api_defaults.background_message_list_limit = value;
        }
        if let Some(value) = self.background_trace_list_limit {
            api_defaults.background_trace_list_limit = value;
        }
        if let Some(value) = self.background_trace_line_limit {
            api_defaults.background_trace_line_limit = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SystemConfigOverride {
    pub worker_count: Option<usize>,
    pub task_timeout_seconds: Option<u64>,
    pub stall_timeout_seconds: Option<u64>,
    pub background_api_timeout_seconds: Option<Option<u64>>,
    pub chat_response_timeout_seconds: Option<Option<u64>>,
    pub max_retries: Option<u32>,
    pub chat_session_retention_days: Option<u32>,
    pub background_task_retention_days: Option<u32>,
    pub checkpoint_retention_days: Option<u32>,
    pub memory_chunk_retention_days: Option<u32>,
    pub log_file_retention_days: Option<u32>,
    pub experimental_features: Option<Vec<String>>,
    pub agent: Option<AgentDefaultsOverride>,
    pub api_defaults: Option<ApiDefaultsOverride>,
}

impl SystemConfigOverride {
    fn apply_to(&self, config: &mut SystemConfig) {
        if let Some(value) = self.worker_count {
            config.worker_count = value;
        }
        if let Some(value) = self.task_timeout_seconds {
            config.task_timeout_seconds = value;
        }
        if let Some(value) = self.stall_timeout_seconds {
            config.stall_timeout_seconds = value;
        }
        if let Some(value) = self.background_api_timeout_seconds {
            config.background_api_timeout_seconds = value;
        }
        if let Some(value) = self.chat_response_timeout_seconds {
            config.chat_response_timeout_seconds = value;
        }
        if let Some(value) = self.max_retries {
            config.max_retries = value;
        }
        if let Some(value) = self.chat_session_retention_days {
            config.chat_session_retention_days = value;
        }
        if let Some(value) = self.background_task_retention_days {
            config.background_task_retention_days = value;
        }
        if let Some(value) = self.checkpoint_retention_days {
            config.checkpoint_retention_days = value;
        }
        if let Some(value) = self.memory_chunk_retention_days {
            config.memory_chunk_retention_days = value;
        }
        if let Some(value) = self.log_file_retention_days {
            config.log_file_retention_days = value;
        }
        if let Some(values) = self.experimental_features.clone() {
            config.experimental_features = values;
        }
        if let Some(agent_override) = &self.agent {
            agent_override.apply_to(&mut config.agent);
        }
        if let Some(api_defaults_override) = &self.api_defaults {
            api_defaults_override.apply_to(&mut config.api_defaults);
        }
    }
}

fn load_config_override(path: &Path) -> Result<Option<SystemConfigOverride>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read system config override from {}",
            path.display()
        )
    })?;
    let parsed: SystemConfigOverride = toml::from_str(&contents).with_context(|| {
        format!(
            "Failed to parse system config override from {}",
            path.display()
        )
    })?;
    Ok(Some(parsed))
}

fn env_override_path(var: &str) -> Option<PathBuf> {
    match env::var_os(var) {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct ResolvedOverridePath {
    path: PathBuf,
    from_env: bool,
}

/// Metadata for a resolved configuration override path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigSourcePathInfo {
    pub path: String,
    pub exists: bool,
    pub from_env: bool,
}

/// Effective configuration source information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveConfigSources {
    pub global: Option<ConfigSourcePathInfo>,
    pub workspace: Option<ConfigSourcePathInfo>,
}

fn global_config_path() -> Option<ResolvedOverridePath> {
    if let Some(path) = env_override_path(GLOBAL_CONFIG_ENV) {
        return Some(ResolvedOverridePath {
            path,
            from_env: true,
        });
    }
    crate::paths::resolve_restflow_dir()
        .ok()
        .map(|dir| ResolvedOverridePath {
            path: dir.join(CONFIG_FILE_NAME),
            from_env: false,
        })
}

fn workspace_config_path() -> Option<ResolvedOverridePath> {
    if let Some(path) = env_override_path(WORKSPACE_CONFIG_ENV) {
        return Some(ResolvedOverridePath {
            path,
            from_env: true,
        });
    }
    env::current_dir().ok().map(|dir| ResolvedOverridePath {
        path: dir.join(CONFIG_SUBDIR).join(CONFIG_FILE_NAME),
        from_env: false,
    })
}

fn collect_override_paths() -> Vec<ResolvedOverridePath> {
    let mut paths: Vec<ResolvedOverridePath> = Vec::new();

    if let Some(global) = global_config_path() {
        paths.push(global);
    }

    if let Some(workspace) = workspace_config_path() {
        let duplicate = paths
            .last()
            .map(|existing| existing.path == workspace.path)
            .unwrap_or(false);
        if !duplicate {
            paths.push(workspace);
        }
    }

    paths
}

fn path_info(resolved: Option<ResolvedOverridePath>) -> Option<ConfigSourcePathInfo> {
    resolved.map(|entry| ConfigSourcePathInfo {
        path: entry.path.display().to_string(),
        exists: entry.path.exists(),
        from_env: entry.from_env,
    })
}

/// Resolve the current effective config source paths and whether they exist.
pub fn effective_config_sources() -> EffectiveConfigSources {
    EffectiveConfigSources {
        global: path_info(global_config_path()),
        workspace: path_info(workspace_config_path()),
    }
}

/// Configuration storage
#[derive(Clone)]
pub struct ConfigStorage {
    db: Arc<Database>,
}

impl ConfigStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table
        let write_txn = db.begin_write()?;
        write_txn.open_table(CONFIG_TABLE)?;
        write_txn.commit()?;

        let storage = Self { db };

        // Set default config if not exists
        if storage.get_config()?.is_none() {
            storage.update_config(SystemConfig::default())?;
        }

        Ok(storage)
    }

    /// Get system configuration
    pub fn get_config(&self) -> Result<Option<SystemConfig>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONFIG_TABLE)?;

        if let Some(data) = table.get("system")? {
            let config: SystemConfig = serde_json::from_slice(data.value())?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Get the effective configuration by applying on-disk overrides to the stored values.
    pub fn get_effective_config(&self) -> Result<SystemConfig> {
        let mut config = self.get_config()?.unwrap_or_default();

        for resolved in collect_override_paths() {
            if let Some(override_config) = load_config_override(&resolved.path)? {
                override_config.apply_to(&mut config);
            }
        }

        config.validate()?;
        Ok(config)
    }

    /// Update system configuration
    pub fn update_config(&self, config: SystemConfig) -> Result<()> {
        // Validate before saving
        config.validate()?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONFIG_TABLE)?;
            let serialized = serde_json::to_vec(&config)?;
            table.insert("system", serialized.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get worker count
    pub fn get_worker_count(&self) -> Result<usize> {
        Ok(self.get_effective_config()?.worker_count)
    }

    /// Update worker count
    pub fn set_worker_count(&self, count: usize) -> Result<()> {
        let mut config = self.get_config()?.unwrap_or_default();
        config.worker_count = count.max(MIN_WORKER_COUNT);
        self.update_config(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use std::path::Path;
    use tempfile::NamedTempFile;
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set_path(key: &'static str, path: &Path) -> Self {
            let original = env::var_os(key);
            // `env` writes are marked unsafe under the `unsafe_env` experiment.
            unsafe {
                env::set_var(key, path);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                unsafe {
                    env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    fn write_override_file(contents: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), contents).unwrap();
        file
    }

    fn setup_test_storage() -> (ConfigStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ConfigStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_default_config() {
        let (storage, _temp_dir) = setup_test_storage();

        let config = storage.get_config().unwrap();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.worker_count, DEFAULT_WORKER_COUNT);
        assert_eq!(config.task_timeout_seconds, DEFAULT_TASK_TIMEOUT_SECONDS);
        assert_eq!(config.background_api_timeout_seconds, None);
        assert_eq!(config.chat_response_timeout_seconds, None);
        assert_eq!(
            config.agent.browser_timeout_secs,
            DEFAULT_BROWSER_TIMEOUT_SECS
        );
        assert_eq!(
            config.agent.process_session_ttl_secs,
            DEFAULT_PROCESS_SESSION_TTL_SECS
        );
        assert_eq!(
            config.agent.approval_timeout_secs,
            DEFAULT_APPROVAL_TIMEOUT_SECS
        );
        assert_eq!(config.agent.max_wall_clock_secs, None);
    }

    #[test]
    fn test_update_config() {
        let (storage, _temp_dir) = setup_test_storage();

        let new_config = SystemConfig {
            worker_count: 8,
            task_timeout_seconds: 600,
            stall_timeout_seconds: 600,
            background_api_timeout_seconds: Some(3600),
            chat_response_timeout_seconds: Some(900),
            max_retries: 5,
            chat_session_retention_days: 45,
            background_task_retention_days: 14,
            checkpoint_retention_days: 5,
            memory_chunk_retention_days: 120,
            experimental_features: vec!["plan_mode".to_string()],
            ..Default::default()
        };

        storage.update_config(new_config).unwrap();

        let retrieved = storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.worker_count, 8);
        assert_eq!(retrieved.task_timeout_seconds, 600);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = SystemConfig {
            worker_count: 2,
            task_timeout_seconds: 30,
            stall_timeout_seconds: 30,
            background_api_timeout_seconds: Some(1200),
            chat_response_timeout_seconds: Some(300),
            max_retries: 1,
            chat_session_retention_days: 30,
            background_task_retention_days: 7,
            checkpoint_retention_days: 3,
            memory_chunk_retention_days: 90,
            experimental_features: vec!["websocket_transport".to_string()],
            ..Default::default()
        };
        assert!(valid_config.validate().is_ok());
    }

    #[test]
    fn test_optional_timeouts_allow_none() {
        let config = SystemConfig {
            background_api_timeout_seconds: None,
            chat_response_timeout_seconds: None,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_chat_response_timeout() {
        let config = SystemConfig {
            chat_response_timeout_seconds: Some(5),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Chat response timeout must be at least")
        );
    }

    #[test]
    fn test_invalid_worker_count() {
        let (storage, _temp_dir) = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 0,
            ..Default::default()
        };

        let result = storage.update_config(invalid_config);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_experimental_features_duplicates() {
        let config = SystemConfig {
            experimental_features: vec!["Plan_Mode".to_string(), "plan_mode".to_string()],
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Duplicate experimental feature")
        );
    }

    #[test]
    fn test_agent_defaults_round_trip() {
        let (storage, _temp_dir) = setup_test_storage();

        let mut config = storage.get_config().unwrap().unwrap();
        assert_eq!(config.agent.tool_timeout_secs, 300);
        assert_eq!(config.agent.bash_timeout_secs, 300);
        assert_eq!(config.agent.max_iterations, 100);
        assert_eq!(config.agent.max_parallel_subagents, 200);
        assert_eq!(
            config.agent.browser_timeout_secs,
            DEFAULT_BROWSER_TIMEOUT_SECS
        );
        assert_eq!(
            config.agent.process_session_ttl_secs,
            DEFAULT_PROCESS_SESSION_TTL_SECS
        );
        assert_eq!(
            config.agent.approval_timeout_secs,
            DEFAULT_APPROVAL_TIMEOUT_SECS
        );

        config.agent.tool_timeout_secs = 180;
        config.agent.bash_timeout_secs = 600;
        config.agent.max_wall_clock_secs = Some(3_600);
        config.agent.max_parallel_subagents = 25;
        config.agent.browser_timeout_secs = 180;
        config.agent.process_session_ttl_secs = 7_200;
        config.agent.approval_timeout_secs = 450;
        storage.update_config(config).unwrap();

        let retrieved = storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.agent.tool_timeout_secs, 180);
        assert_eq!(retrieved.agent.bash_timeout_secs, 600);
        assert_eq!(retrieved.agent.max_wall_clock_secs, Some(3_600));
        assert_eq!(retrieved.agent.max_parallel_subagents, 25);
        assert_eq!(retrieved.agent.browser_timeout_secs, 180);
        assert_eq!(retrieved.agent.process_session_ttl_secs, 7_200);
        assert_eq!(retrieved.agent.approval_timeout_secs, 450);
    }

    #[test]
    fn test_invalid_max_parallel_subagents() {
        let mut config = SystemConfig::default();
        config.agent.max_parallel_subagents = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_log_file_retention_default() {
        let config = SystemConfig::default();
        assert_eq!(config.log_file_retention_days, 30);
    }

    #[test]
    fn test_log_file_retention_validation() {
        // 0 is valid (keep forever)
        let mut config = SystemConfig {
            log_file_retention_days: 0,
            ..SystemConfig::default()
        };
        assert!(config.validate().is_ok());

        // 1 is valid
        config.log_file_retention_days = 1;
        assert!(config.validate().is_ok());

        // 365 is valid
        config.log_file_retention_days = 365;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_agent_defaults_validation() {
        let mut config = SystemConfig::default();
        config.agent.tool_timeout_secs = 5; // below min
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.max_iterations = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.max_wall_clock_secs = Some(5);
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.max_wall_clock_secs = None;
        assert!(config.validate().is_ok());

        let mut config = SystemConfig::default();
        config.agent.browser_timeout_secs = 5;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.process_session_ttl_secs = 5;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.approval_timeout_secs = 5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_effective_config_without_overrides() {
        let (storage, _temp_dir) = setup_test_storage();
        let effective = storage.get_effective_config().unwrap();
        let stored = storage.get_config().unwrap().unwrap();
        assert_eq!(effective.worker_count, stored.worker_count);
    }

    #[test]
    fn test_effective_config_with_global_override() {
        let _env_guard = env_lock();
        let (storage, _temp_dir) = setup_test_storage();
        let file = write_override_file("worker_count = 42\nbackground_task_retention_days = 10");
        let _guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, file.path());

        let effective = storage.get_effective_config().unwrap();
        assert_eq!(effective.worker_count, 42);
        assert_eq!(effective.background_task_retention_days, 10);
    }

    #[test]
    fn test_workspace_override_precedence() {
        let _env_guard = env_lock();
        let (storage, _temp_dir) = setup_test_storage();
        let global_file = write_override_file("worker_count = 5\nmax_retries = 2");
        let workspace_file = write_override_file("worker_count = 9\nmax_retries = 4");
        let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, global_file.path());
        let _workspace_guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, workspace_file.path());

        let effective = storage.get_effective_config().unwrap();
        assert_eq!(effective.worker_count, 9);
        assert_eq!(effective.max_retries, 4);
    }

    #[test]
    fn test_partial_agent_override() {
        let _env_guard = env_lock();
        let (storage, _temp_dir) = setup_test_storage();
        let file = write_override_file(
            r#"task_timeout_seconds = 9999

[agent]
python_timeout_secs = 45
browser_timeout_secs = 240
process_session_ttl_secs = 5400
approval_timeout_secs = 420
max_wall_clock_secs = 7200
fallback_models = ["alpha", "beta"]
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = storage.get_effective_config().unwrap();
        assert_eq!(effective.task_timeout_seconds, 9999);
        assert_eq!(effective.agent.python_timeout_secs, 45);
        assert_eq!(effective.agent.browser_timeout_secs, 240);
        assert_eq!(effective.agent.process_session_ttl_secs, 5400);
        assert_eq!(effective.agent.approval_timeout_secs, 420);
        assert_eq!(effective.agent.max_wall_clock_secs, Some(7200));
        assert_eq!(
            effective.agent.fallback_models,
            Some(vec!["alpha".into(), "beta".into()])
        );
    }

    #[test]
    fn test_effective_config_sources_reports_paths_and_existence() {
        let _env_guard = env_lock();
        let global_file = write_override_file("worker_count = 7");
        let workspace_file = write_override_file("worker_count = 9");
        let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, global_file.path());
        let _workspace_guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, workspace_file.path());

        let sources = effective_config_sources();
        let global = sources.global.expect("global source should exist");
        let workspace = sources.workspace.expect("workspace source should exist");

        assert!(global.exists);
        assert!(workspace.exists);
        assert!(global.from_env);
        assert!(workspace.from_env);
        assert!(global.path.ends_with(global_file.path().to_str().unwrap()));
        assert!(
            workspace
                .path
                .ends_with(workspace_file.path().to_str().unwrap())
        );
    }

    #[test]
    fn test_invalid_override_rejected() {
        let _env_guard = env_lock();
        let (storage, _temp_dir) = setup_test_storage();
        let file = write_override_file("worker_count = 0");
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let result = storage.get_effective_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_api_defaults_round_trip() {
        let (storage, _temp_dir) = setup_test_storage();
        let mut config = storage.get_config().unwrap().unwrap();
        assert_eq!(config.api_defaults.memory_search_limit, 10);
        assert_eq!(config.api_defaults.background_trace_line_limit, 200);

        config.api_defaults.memory_search_limit = 25;
        config.api_defaults.background_trace_line_limit = 300;
        storage.update_config(config).unwrap();

        let retrieved = storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.api_defaults.memory_search_limit, 25);
        assert_eq!(retrieved.api_defaults.background_trace_line_limit, 300);
    }

    #[test]
    fn test_invalid_api_defaults_rejected() {
        let mut config = SystemConfig::default();
        config.api_defaults.background_message_list_limit = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_api_defaults_override_from_file() {
        let _env_guard = env_lock();
        let (storage, _temp_dir) = setup_test_storage();
        let file = write_override_file(
            r#"[api_defaults]
memory_search_limit = 33
background_trace_line_limit = 444
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = storage.get_effective_config().unwrap();
        assert_eq!(effective.api_defaults.memory_search_limit, 33);
        assert_eq!(effective.api_defaults.background_trace_line_limit, 444);
    }
}
