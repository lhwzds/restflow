//! System configuration storage.

use anyhow::{Context, Result};
use redb::Database;
use restflow_traits::{
    DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS, DEFAULT_AGENT_BASH_TIMEOUT_SECS,
    DEFAULT_AGENT_BROWSER_TIMEOUT_SECS, DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS,
    DEFAULT_AGENT_LLM_TIMEOUT_SECS, DEFAULT_AGENT_MAX_DURATION_SECS, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_AGENT_MAX_TOOL_CALLS, DEFAULT_AGENT_MAX_TOOL_CONCURRENCY,
    DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH, DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
    DEFAULT_AGENT_PYTHON_TIMEOUT_SECS, DEFAULT_AGENT_TASK_TIMEOUT_SECS,
    DEFAULT_AGENT_TOOL_TIMEOUT_SECS, DEFAULT_API_DIAGNOSTICS_TIMEOUT_MS,
    DEFAULT_API_WEB_SEARCH_RESULTS, DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS,
    DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS, DEFAULT_BG_MESSAGE_LIST_LIMIT,
    DEFAULT_BG_PROGRESS_EVENT_LIMIT, DEFAULT_BG_TRACE_LINE_LIMIT, DEFAULT_BG_TRACE_LIST_LIMIT,
    DEFAULT_CHAT_MAX_SESSION_HISTORY, DEFAULT_GITHUB_CACHE_TTL_SECS,
    DEFAULT_MARKETPLACE_CACHE_TTL_SECS, DEFAULT_MAX_PARALLEL_SUBAGENTS,
    DEFAULT_PROCESS_SESSION_TTL_SECS, DEFAULT_SUBAGENT_TIMEOUT_SECS,
    DEFAULT_TELEGRAM_API_TIMEOUT_SECS, DEFAULT_TELEGRAM_POLLING_TIMEOUT_SECS,
    MAX_API_WEB_SEARCH_RESULTS,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;
use specta::Type;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const GLOBAL_CONFIG_ENV: &str = "RESTFLOW_GLOBAL_CONFIG";
const WORKSPACE_CONFIG_ENV: &str = "RESTFLOW_WORKSPACE_CONFIG";
const CONFIG_SUBDIR: &str = ".restflow";
const CONFIG_FILE_NAME: &str = "config.toml";
const LEGACY_CONFIG_JSON_FILE_NAME: &str = "config.json";
const LEGACY_CONFIG_TOML_DIR_NAME: &str = "restflow";

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
const MIN_RETENTION_DAYS: u32 = 1;
const MIN_WORKER_COUNT: usize = 1;
const MIN_TIMEOUT_SECONDS: u64 = 10;

fn default_cli_timeout() -> u64 {
    120
}

fn default_cli_max_output() -> usize {
    1_048_576
}

/// CLI-specific defaults stored in the unified config file.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct CliConfig {
    pub version: u32,
    pub default: CliDefaultConfig,
    pub sandbox: CliSandboxConfig,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            version: 1,
            default: CliDefaultConfig::default(),
            sandbox: CliSandboxConfig::default(),
        }
    }
}

impl CliConfig {
    pub fn load() -> Self {
        load_cli_config().unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        write_cli_config(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Type)]
#[serde(default)]
pub struct CliDefaultConfig {
    pub agent: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Type)]
#[serde(default)]
pub struct CliSandboxConfig {
    pub enabled: bool,
    pub env: CliEnvSandboxConfig,
    pub limits: CliLimitsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Type)]
#[serde(default)]
pub struct CliEnvSandboxConfig {
    pub isolate: bool,
    pub allow: Vec<String>,
    pub block: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct CliLimitsConfig {
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
}

impl Default for CliLimitsConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_cli_timeout(),
            max_output_bytes: default_cli_max_output(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct UnifiedConfigFile {
    #[serde(flatten)]
    system: SystemConfig,
    #[serde(default)]
    cli: CliConfig,
}

/// Agent execution defaults (configurable at runtime via `manage_config`).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct AgentDefaults {
    /// Timeout for a single tool execution in seconds.
    pub tool_timeout_secs: u64,
    /// Default timeout for each LLM completion request in seconds.
    ///
    /// `None` disables the per-request LLM timeout.
    pub llm_timeout_secs: Option<u64>,
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
    /// Maximum number of tool calls that may run concurrently.
    pub max_tool_concurrency: usize,
    /// Maximum tool result length kept in the LLM context.
    pub max_tool_result_length: usize,
    /// Maximum characters preserved for pruned historical tool output.
    pub prune_tool_max_chars: usize,
    /// Tokens preserved from the recent tail during context compaction.
    pub compact_preserve_tokens: usize,
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
    #[serde(default)]
    pub fallback_models: Option<Vec<String>>,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            tool_timeout_secs: DEFAULT_AGENT_TOOL_TIMEOUT_SECS,
            llm_timeout_secs: Some(DEFAULT_AGENT_LLM_TIMEOUT_SECS),
            bash_timeout_secs: DEFAULT_AGENT_BASH_TIMEOUT_SECS,
            python_timeout_secs: DEFAULT_AGENT_PYTHON_TIMEOUT_SECS,
            browser_timeout_secs: DEFAULT_AGENT_BROWSER_TIMEOUT_SECS,
            process_session_ttl_secs: DEFAULT_PROCESS_SESSION_TTL_SECS,
            approval_timeout_secs: DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS,
            max_iterations: DEFAULT_AGENT_MAX_ITERATIONS,
            subagent_timeout_secs: DEFAULT_SUBAGENT_TIMEOUT_SECS,
            max_parallel_subagents: DEFAULT_MAX_PARALLEL_SUBAGENTS,
            max_tool_calls: DEFAULT_AGENT_MAX_TOOL_CALLS,
            max_tool_concurrency: DEFAULT_AGENT_MAX_TOOL_CONCURRENCY,
            max_tool_result_length: DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH,
            prune_tool_max_chars: DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
            compact_preserve_tokens: DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS,
            max_wall_clock_secs: None,
            default_task_timeout_secs: DEFAULT_AGENT_TASK_TIMEOUT_SECS,
            default_max_duration_secs: DEFAULT_AGENT_MAX_DURATION_SECS,
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
        if let Some(timeout_secs) = self.llm_timeout_secs
            && timeout_secs < MIN_TIMEOUT_SECONDS
        {
            return Err(anyhow::anyhow!(
                "agent.llm_timeout_secs must be at least {} seconds",
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
        if self.max_tool_concurrency == 0 {
            return Err(anyhow::anyhow!(
                "agent.max_tool_concurrency must be at least 1"
            ));
        }
        if self.max_tool_result_length == 0 {
            return Err(anyhow::anyhow!(
                "agent.max_tool_result_length must be at least 1"
            ));
        }
        if self.prune_tool_max_chars == 0 {
            return Err(anyhow::anyhow!(
                "agent.prune_tool_max_chars must be at least 1"
            ));
        }
        if self.compact_preserve_tokens == 0 {
            return Err(anyhow::anyhow!(
                "agent.compact_preserve_tokens must be at least 1"
            ));
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
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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
    /// Default result count for `web_search`.
    pub web_search_num_results: usize,
    /// Default diagnostics wait timeout in milliseconds.
    pub diagnostics_timeout_ms: u64,
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
            web_search_num_results: DEFAULT_API_WEB_SEARCH_RESULTS,
            diagnostics_timeout_ms: DEFAULT_API_DIAGNOSTICS_TIMEOUT_MS,
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
        if self.web_search_num_results == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.web_search_num_results must be at least 1"
            ));
        }
        if self.web_search_num_results > MAX_API_WEB_SEARCH_RESULTS {
            return Err(anyhow::anyhow!(
                "api_defaults.web_search_num_results must be at most {}",
                MAX_API_WEB_SEARCH_RESULTS
            ));
        }
        if self.diagnostics_timeout_ms == 0 {
            return Err(anyhow::anyhow!(
                "api_defaults.diagnostics_timeout_ms must be at least 1"
            ));
        }
        Ok(())
    }
}

/// Runtime execution defaults for daemon/background/chat behavior.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct RuntimeDefaults {
    /// Background runner poll interval in milliseconds.
    pub background_runner_poll_interval_ms: u64,
    /// Maximum concurrent tasks for the background runner.
    pub background_runner_max_concurrent_tasks: usize,
    /// Maximum session history kept for channel chat sessions.
    pub chat_max_session_history: usize,
}

impl Default for RuntimeDefaults {
    fn default() -> Self {
        Self {
            background_runner_poll_interval_ms: DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS,
            background_runner_max_concurrent_tasks: DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS,
            chat_max_session_history: DEFAULT_CHAT_MAX_SESSION_HISTORY,
        }
    }
}

impl RuntimeDefaults {
    fn validate(&self) -> Result<()> {
        if self.background_runner_poll_interval_ms == 0 {
            return Err(anyhow::anyhow!(
                "runtime_defaults.background_runner_poll_interval_ms must be at least 1"
            ));
        }
        if self.background_runner_max_concurrent_tasks == 0 {
            return Err(anyhow::anyhow!(
                "runtime_defaults.background_runner_max_concurrent_tasks must be at least 1"
            ));
        }
        if self.chat_max_session_history == 0 {
            return Err(anyhow::anyhow!(
                "runtime_defaults.chat_max_session_history must be at least 1"
            ));
        }
        Ok(())
    }
}

/// Channel-specific defaults for external integrations.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct ChannelDefaults {
    /// Timeout for Telegram API HTTP calls in seconds.
    pub telegram_api_timeout_secs: u64,
    /// Telegram long-poll timeout in seconds.
    pub telegram_polling_timeout_secs: u32,
}

impl Default for ChannelDefaults {
    fn default() -> Self {
        Self {
            telegram_api_timeout_secs: DEFAULT_TELEGRAM_API_TIMEOUT_SECS,
            telegram_polling_timeout_secs: DEFAULT_TELEGRAM_POLLING_TIMEOUT_SECS,
        }
    }
}

impl ChannelDefaults {
    fn validate(&self) -> Result<()> {
        if self.telegram_api_timeout_secs < MIN_TIMEOUT_SECONDS {
            return Err(anyhow::anyhow!(
                "channel_defaults.telegram_api_timeout_secs must be at least {} seconds",
                MIN_TIMEOUT_SECONDS
            ));
        }
        if self.telegram_polling_timeout_secs == 0 {
            return Err(anyhow::anyhow!(
                "channel_defaults.telegram_polling_timeout_secs must be at least 1"
            ));
        }
        Ok(())
    }
}

/// Registry and marketplace integration defaults.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct RegistryDefaults {
    /// GitHub provider cache TTL in seconds.
    pub github_cache_ttl_secs: u64,
    /// Marketplace provider cache TTL in seconds.
    pub marketplace_cache_ttl_secs: u64,
}

impl Default for RegistryDefaults {
    fn default() -> Self {
        Self {
            github_cache_ttl_secs: DEFAULT_GITHUB_CACHE_TTL_SECS,
            marketplace_cache_ttl_secs: DEFAULT_MARKETPLACE_CACHE_TTL_SECS,
        }
    }
}

impl RegistryDefaults {
    fn validate(&self) -> Result<()> {
        if self.github_cache_ttl_secs == 0 {
            return Err(anyhow::anyhow!(
                "registry_defaults.github_cache_ttl_secs must be at least 1"
            ));
        }
        if self.marketplace_cache_ttl_secs == 0 {
            return Err(anyhow::anyhow!(
                "registry_defaults.marketplace_cache_ttl_secs must be at least 1"
            ));
        }
        Ok(())
    }
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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
    /// Runtime execution defaults for daemon/background/chat services.
    #[serde(default)]
    pub runtime_defaults: RuntimeDefaults,
    /// Channel integration defaults.
    #[serde(default)]
    pub channel_defaults: ChannelDefaults,
    /// Registry provider defaults.
    #[serde(default)]
    pub registry_defaults: RegistryDefaults,
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
            runtime_defaults: RuntimeDefaults::default(),
            channel_defaults: ChannelDefaults::default(),
            registry_defaults: RegistryDefaults::default(),
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
        self.runtime_defaults.validate()?;
        self.channel_defaults.validate()?;
        self.registry_defaults.validate()?;

        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CliDefaultConfigOverride {
    pub agent: Option<String>,
    pub model: Option<String>,
}

impl CliDefaultConfigOverride {
    fn apply_to(&self, config: &mut CliDefaultConfig) {
        if let Some(value) = self.agent.clone() {
            config.agent = Some(value);
        }
        if let Some(value) = self.model.clone() {
            config.model = Some(value);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CliEnvSandboxConfigOverride {
    pub isolate: Option<bool>,
    pub allow: Option<Vec<String>>,
    pub block: Option<Vec<String>>,
}

impl CliEnvSandboxConfigOverride {
    fn apply_to(&self, config: &mut CliEnvSandboxConfig) {
        if let Some(value) = self.isolate {
            config.isolate = value;
        }
        if let Some(value) = self.allow.clone() {
            config.allow = value;
        }
        if let Some(value) = self.block.clone() {
            config.block = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CliLimitsConfigOverride {
    pub timeout_secs: Option<u64>,
    pub max_output_bytes: Option<usize>,
}

impl CliLimitsConfigOverride {
    fn apply_to(&self, config: &mut CliLimitsConfig) {
        if let Some(value) = self.timeout_secs {
            config.timeout_secs = value;
        }
        if let Some(value) = self.max_output_bytes {
            config.max_output_bytes = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CliSandboxConfigOverride {
    pub enabled: Option<bool>,
    pub env: Option<CliEnvSandboxConfigOverride>,
    pub limits: Option<CliLimitsConfigOverride>,
}

impl CliSandboxConfigOverride {
    fn apply_to(&self, config: &mut CliSandboxConfig) {
        if let Some(value) = self.enabled {
            config.enabled = value;
        }
        if let Some(value) = &self.env {
            value.apply_to(&mut config.env);
        }
        if let Some(value) = &self.limits {
            value.apply_to(&mut config.limits);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CliConfigOverride {
    pub version: Option<u32>,
    pub default: Option<CliDefaultConfigOverride>,
    pub sandbox: Option<CliSandboxConfigOverride>,
}

impl CliConfigOverride {
    fn apply_to(&self, config: &mut CliConfig) {
        if let Some(value) = self.version {
            config.version = value;
        }
        if let Some(value) = &self.default {
            value.apply_to(&mut config.default);
        }
        if let Some(value) = &self.sandbox {
            value.apply_to(&mut config.sandbox);
        }
    }
}

fn deserialize_optional_u64_override<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<u64>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ValueOrClear {
        Value(u64),
        Clear(String),
    }

    let parsed = Option::<ValueOrClear>::deserialize(deserializer)?;
    Ok(match parsed {
        None => None,
        Some(ValueOrClear::Value(value)) => Some(Some(value)),
        Some(ValueOrClear::Clear(value)) => {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "none" | "null" | "unset" => Some(None),
                _ => {
                    return Err(serde::de::Error::custom(
                        "expected a number or one of: \"none\", \"null\", \"unset\"",
                    ));
                }
            }
        }
    })
}

fn deserialize_optional_string_list_override<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<Vec<String>>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ValueOrClear {
        Values(Vec<String>),
        Clear(String),
    }

    let parsed = Option::<ValueOrClear>::deserialize(deserializer)?;
    Ok(match parsed {
        None => None,
        Some(ValueOrClear::Values(values)) => Some(Some(values)),
        Some(ValueOrClear::Clear(value)) => {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "none" | "null" | "unset" => Some(None),
                _ => {
                    return Err(serde::de::Error::custom(
                        "expected an array of strings or one of: \"none\", \"null\", \"unset\"",
                    ));
                }
            }
        }
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AgentDefaultsOverride {
    pub tool_timeout_secs: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_optional_u64_override")]
    pub llm_timeout_secs: Option<Option<u64>>,
    pub bash_timeout_secs: Option<u64>,
    pub python_timeout_secs: Option<u64>,
    pub browser_timeout_secs: Option<u64>,
    pub process_session_ttl_secs: Option<u64>,
    pub approval_timeout_secs: Option<u64>,
    pub max_iterations: Option<usize>,
    pub subagent_timeout_secs: Option<u64>,
    pub max_parallel_subagents: Option<usize>,
    pub max_tool_calls: Option<usize>,
    pub max_tool_concurrency: Option<usize>,
    pub max_tool_result_length: Option<usize>,
    pub prune_tool_max_chars: Option<usize>,
    pub compact_preserve_tokens: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_optional_u64_override")]
    pub max_wall_clock_secs: Option<Option<u64>>,
    pub default_task_timeout_secs: Option<u64>,
    pub default_max_duration_secs: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_string_list_override"
    )]
    pub fallback_models: Option<Option<Vec<String>>>,
}

impl AgentDefaultsOverride {
    fn apply_to(&self, agent: &mut AgentDefaults) {
        if let Some(value) = self.tool_timeout_secs {
            agent.tool_timeout_secs = value;
        }
        if let Some(value) = self.llm_timeout_secs {
            agent.llm_timeout_secs = value;
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
        if let Some(value) = self.max_tool_concurrency {
            agent.max_tool_concurrency = value;
        }
        if let Some(value) = self.max_tool_result_length {
            agent.max_tool_result_length = value;
        }
        if let Some(value) = self.prune_tool_max_chars {
            agent.prune_tool_max_chars = value;
        }
        if let Some(value) = self.compact_preserve_tokens {
            agent.compact_preserve_tokens = value;
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
#[serde(default, deny_unknown_fields)]
struct ApiDefaultsOverride {
    pub memory_search_limit: Option<u32>,
    pub session_list_limit: Option<u32>,
    pub background_progress_event_limit: Option<usize>,
    pub background_message_list_limit: Option<usize>,
    pub background_trace_list_limit: Option<usize>,
    pub background_trace_line_limit: Option<usize>,
    pub web_search_num_results: Option<usize>,
    pub diagnostics_timeout_ms: Option<u64>,
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
        if let Some(value) = self.web_search_num_results {
            api_defaults.web_search_num_results = value;
        }
        if let Some(value) = self.diagnostics_timeout_ms {
            api_defaults.diagnostics_timeout_ms = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RuntimeDefaultsOverride {
    pub background_runner_poll_interval_ms: Option<u64>,
    pub background_runner_max_concurrent_tasks: Option<usize>,
    pub chat_max_session_history: Option<usize>,
}

impl RuntimeDefaultsOverride {
    fn apply_to(&self, runtime_defaults: &mut RuntimeDefaults) {
        if let Some(value) = self.background_runner_poll_interval_ms {
            runtime_defaults.background_runner_poll_interval_ms = value;
        }
        if let Some(value) = self.background_runner_max_concurrent_tasks {
            runtime_defaults.background_runner_max_concurrent_tasks = value;
        }
        if let Some(value) = self.chat_max_session_history {
            runtime_defaults.chat_max_session_history = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ChannelDefaultsOverride {
    pub telegram_api_timeout_secs: Option<u64>,
    pub telegram_polling_timeout_secs: Option<u32>,
}

impl ChannelDefaultsOverride {
    fn apply_to(&self, channel_defaults: &mut ChannelDefaults) {
        if let Some(value) = self.telegram_api_timeout_secs {
            channel_defaults.telegram_api_timeout_secs = value;
        }
        if let Some(value) = self.telegram_polling_timeout_secs {
            channel_defaults.telegram_polling_timeout_secs = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RegistryDefaultsOverride {
    pub github_cache_ttl_secs: Option<u64>,
    pub marketplace_cache_ttl_secs: Option<u64>,
}

impl RegistryDefaultsOverride {
    fn apply_to(&self, registry_defaults: &mut RegistryDefaults) {
        if let Some(value) = self.github_cache_ttl_secs {
            registry_defaults.github_cache_ttl_secs = value;
        }
        if let Some(value) = self.marketplace_cache_ttl_secs {
            registry_defaults.marketplace_cache_ttl_secs = value;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SystemConfigOverride {
    pub worker_count: Option<usize>,
    pub task_timeout_seconds: Option<u64>,
    pub stall_timeout_seconds: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_optional_u64_override")]
    pub background_api_timeout_seconds: Option<Option<u64>>,
    #[serde(default, deserialize_with = "deserialize_optional_u64_override")]
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
    pub runtime_defaults: Option<RuntimeDefaultsOverride>,
    pub channel_defaults: Option<ChannelDefaultsOverride>,
    pub registry_defaults: Option<RegistryDefaultsOverride>,
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
        if let Some(runtime_defaults_override) = &self.runtime_defaults {
            runtime_defaults_override.apply_to(&mut config.runtime_defaults);
        }
        if let Some(channel_defaults_override) = &self.channel_defaults {
            channel_defaults_override.apply_to(&mut config.channel_defaults);
        }
        if let Some(registry_defaults_override) = &self.registry_defaults {
            registry_defaults_override.apply_to(&mut config.registry_defaults);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UnifiedConfigOverride {
    #[serde(flatten)]
    pub system: SystemConfigOverride,
    pub cli: Option<CliConfigOverride>,
}

impl UnifiedConfigOverride {
    fn apply_to(&self, config: &mut UnifiedConfigFile) {
        self.system.apply_to(&mut config.system);
        if let Some(cli_override) = &self.cli {
            cli_override.apply_to(&mut config.cli);
        }
    }
}

fn load_config_override(path: &Path) -> Result<Option<UnifiedConfigOverride>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read system config override from {}",
            path.display()
        )
    })?;
    let parsed: UnifiedConfigOverride = toml::from_str(&contents).with_context(|| {
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

/// Source of an effective configuration value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigValueSourceKind {
    Default,
    Global,
    Workspace,
}

/// Per-key source information for effective config values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigValueSourceInfo {
    pub source: ConfigValueSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Effective configuration source information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveConfigSources {
    pub global: Option<ConfigSourcePathInfo>,
    pub workspace: Option<ConfigSourcePathInfo>,
    pub write_target: Option<ConfigSourcePathInfo>,
    pub values: BTreeMap<String, ConfigValueSourceInfo>,
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

fn path_info(resolved: Option<ResolvedOverridePath>) -> Option<ConfigSourcePathInfo> {
    resolved.map(|entry| ConfigSourcePathInfo {
        path: entry.path.display().to_string(),
        exists: entry.path.exists(),
        from_env: entry.from_env,
    })
}

fn global_write_target() -> Result<ResolvedOverridePath> {
    global_config_path().ok_or_else(|| anyhow::anyhow!("Failed to resolve global config.toml path"))
}

fn legacy_cli_config_path() -> Option<PathBuf> {
    crate::paths::resolve_restflow_dir()
        .ok()
        .map(|dir| dir.join(LEGACY_CONFIG_JSON_FILE_NAME))
}

fn legacy_cli_toml_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(LEGACY_CONFIG_TOML_DIR_NAME).join(CONFIG_FILE_NAME))
}

fn read_legacy_cli_config() -> Result<Option<CliConfig>> {
    if let Some(cli) = read_legacy_cli_json_config()? {
        return Ok(Some(cli));
    }
    read_legacy_cli_toml_config()
}

fn read_legacy_cli_json_config() -> Result<Option<CliConfig>> {
    let Some(path) = legacy_cli_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read legacy CLI config from {}", path.display()))?;
    let cli = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse legacy CLI config from {}", path.display()))?;
    Ok(Some(cli))
}

fn read_legacy_cli_toml_config() -> Result<Option<CliConfig>> {
    #[derive(Debug, Deserialize)]
    struct LegacyCliTomlConfig {
        default: Option<LegacyCliTomlDefaultConfig>,
    }

    #[derive(Debug, Deserialize)]
    struct LegacyCliTomlDefaultConfig {
        agent: Option<String>,
        model: Option<String>,
        #[allow(dead_code)]
        db_path: Option<String>,
    }

    let Some(path) = legacy_cli_toml_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read legacy CLI config from {}", path.display()))?;
    let legacy: LegacyCliTomlConfig = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse legacy CLI config from {}", path.display()))?;

    Ok(Some(CliConfig {
        version: 1,
        default: CliDefaultConfig {
            agent: legacy
                .default
                .as_ref()
                .and_then(|value| value.agent.clone()),
            model: legacy.default.and_then(|value| value.model),
        },
        sandbox: CliSandboxConfig::default(),
    }))
}

#[derive(Debug, Clone)]
struct ConfigLayerState {
    default: UnifiedConfigFile,
    global: UnifiedConfigFile,
    effective: UnifiedConfigFile,
    global_path: Option<ResolvedOverridePath>,
    workspace_path: Option<ResolvedOverridePath>,
}

fn load_config_layers() -> Result<ConfigLayerState> {
    let default = UnifiedConfigFile::default();
    let global_path = global_config_path();
    let workspace_path = workspace_config_path();

    let mut global = default.clone();
    if let Some(path) = global_path.as_ref()
        && let Some(override_config) = load_config_override(&path.path)?
    {
        override_config.apply_to(&mut global);
    }

    let mut effective = global.clone();
    if let Some(path) = workspace_path.as_ref()
        && let Some(override_config) = load_config_override(&path.path)?
    {
        override_config.apply_to(&mut effective);
    }

    global.system.validate()?;
    effective.system.validate()?;

    Ok(ConfigLayerState {
        default,
        global,
        effective,
        global_path,
        workspace_path,
    })
}

fn write_global_config_file(config: &UnifiedConfigFile) -> Result<()> {
    config.system.validate()?;
    let target = global_write_target()?;
    if let Some(parent) = target.path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
    }
    let contents = toml::to_string_pretty(config).context("Failed to serialize config.toml")?;
    fs::write(&target.path, contents)
        .with_context(|| format!("Failed to write config.toml to {}", target.path.display()))?;
    Ok(())
}

fn flatten_json(prefix: Option<&str>, value: &JsonValue, output: &mut BTreeMap<String, JsonValue>) {
    match value {
        JsonValue::Object(map) => {
            for (key, entry) in map {
                let next = match prefix {
                    Some(prefix) => format!("{prefix}.{key}"),
                    None => key.clone(),
                };
                flatten_json(Some(&next), entry, output);
            }
        }
        _ => {
            if let Some(prefix) = prefix {
                output.insert(prefix.to_string(), value.clone());
            }
        }
    }
}

fn flatten_system_config(config: &SystemConfig) -> Result<BTreeMap<String, JsonValue>> {
    let json = serde_json::to_value(config).context("Failed to serialize system config")?;
    let mut output = BTreeMap::new();
    flatten_json(None, &json, &mut output);
    Ok(output)
}

fn build_value_sources(
    layers: &ConfigLayerState,
) -> Result<BTreeMap<String, ConfigValueSourceInfo>> {
    let default_values = flatten_system_config(&layers.default.system)?;
    let global_values = flatten_system_config(&layers.global.system)?;
    let effective_values = flatten_system_config(&layers.effective.system)?;

    let global_path = layers
        .global_path
        .as_ref()
        .map(|path| path.path.display().to_string());
    let workspace_path = layers
        .workspace_path
        .as_ref()
        .map(|path| path.path.display().to_string());

    let mut values = BTreeMap::new();
    for (key, final_value) in effective_values {
        let default_value = default_values.get(&key);
        let global_value = global_values.get(&key);

        let info = if global_value != Some(&final_value) {
            ConfigValueSourceInfo {
                source: ConfigValueSourceKind::Workspace,
                path: workspace_path.clone(),
            }
        } else if default_value != Some(&final_value) {
            ConfigValueSourceInfo {
                source: ConfigValueSourceKind::Global,
                path: global_path.clone(),
            }
        } else {
            ConfigValueSourceInfo {
                source: ConfigValueSourceKind::Default,
                path: None,
            }
        };
        values.insert(key, info);
    }

    Ok(values)
}

fn migrate_legacy_cli_config_if_needed() -> Result<()> {
    let Some(global) = global_config_path() else {
        return Ok(());
    };
    if global.path.exists() {
        return Ok(());
    }

    let Some(cli) = read_legacy_cli_config()? else {
        return Ok(());
    };

    let unified = UnifiedConfigFile {
        cli,
        ..UnifiedConfigFile::default()
    };
    write_global_config_file(&unified)
}

pub fn load_cli_config() -> Result<CliConfig> {
    migrate_legacy_cli_config_if_needed()?;
    Ok(load_config_layers()?.effective.cli)
}

pub fn write_cli_config(config: &CliConfig) -> Result<()> {
    let mut current = load_config_layers()
        .map(|layers| layers.global)
        .unwrap_or_default();
    current.cli = config.clone();
    write_global_config_file(&current)
}

/// Resolve the current effective config source paths and whether they exist.
pub fn effective_config_sources() -> Result<EffectiveConfigSources> {
    let layers = load_config_layers()?;
    Ok(EffectiveConfigSources {
        global: path_info(layers.global_path.clone()),
        workspace: path_info(layers.workspace_path.clone()),
        write_target: path_info(global_config_path()),
        values: build_value_sources(&layers)?,
    })
}

/// Configuration storage
#[derive(Clone, Default)]
pub struct ConfigStorage;

impl ConfigStorage {
    pub fn new(_db: Arc<Database>) -> Result<Self> {
        Ok(Self)
    }

    /// Get the global config view (defaults + global config.toml).
    pub fn get_config(&self) -> Result<Option<SystemConfig>> {
        self.get_global_config().map(Some)
    }

    /// Get the global config view (defaults + global config.toml).
    pub fn get_global_config(&self) -> Result<SystemConfig> {
        Ok(load_config_layers()?.global.system)
    }

    /// Get the effective configuration by applying config.toml overrides.
    pub fn get_effective_config(&self) -> Result<SystemConfig> {
        Ok(load_config_layers()?.effective.system)
    }

    /// Update the global config.toml system configuration while preserving the CLI section.
    pub fn update_config(&self, config: SystemConfig) -> Result<()> {
        config.validate()?;
        let mut current = load_config_layers()
            .map(|layers| layers.global)
            .unwrap_or_default();
        current.system = config;
        write_global_config_file(&current)?;
        Ok(())
    }

    /// Get worker count
    pub fn get_worker_count(&self) -> Result<usize> {
        Ok(self.get_effective_config()?.worker_count)
    }

    /// Update worker count
    pub fn set_worker_count(&self, count: usize) -> Result<()> {
        let mut config = self.get_global_config()?;
        config.worker_count = count.max(MIN_WORKER_COUNT);
        self.update_config(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
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

    struct TestContext {
        storage: ConfigStorage,
        _temp_dir: tempfile::TempDir,
        _env_guard: std::sync::MutexGuard<'static, ()>,
        _global_guard: EnvGuard,
    }

    fn setup_test_storage() -> TestContext {
        let env_guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, &config_path);
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ConfigStorage::new(db).unwrap();
        TestContext {
            storage,
            _temp_dir: temp_dir,
            _env_guard: env_guard,
            _global_guard: global_guard,
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_default_config() {
        let ctx = setup_test_storage();

        let config = ctx.storage.get_config().unwrap();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.worker_count, DEFAULT_WORKER_COUNT);
        assert_eq!(config.task_timeout_seconds, DEFAULT_TASK_TIMEOUT_SECONDS);
        assert_eq!(config.background_api_timeout_seconds, None);
        assert_eq!(config.chat_response_timeout_seconds, None);
        assert_eq!(
            config.agent.browser_timeout_secs,
            DEFAULT_AGENT_BROWSER_TIMEOUT_SECS
        );
        assert_eq!(
            config.agent.process_session_ttl_secs,
            DEFAULT_PROCESS_SESSION_TTL_SECS
        );
        assert_eq!(
            config.agent.approval_timeout_secs,
            DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS
        );
        assert_eq!(
            config.agent.llm_timeout_secs,
            Some(DEFAULT_AGENT_LLM_TIMEOUT_SECS)
        );
        assert_eq!(
            config.agent.max_tool_concurrency,
            DEFAULT_AGENT_MAX_TOOL_CONCURRENCY
        );
        assert_eq!(
            config.agent.max_tool_result_length,
            DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH
        );
        assert_eq!(
            config.agent.prune_tool_max_chars,
            DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS
        );
        assert_eq!(
            config.agent.compact_preserve_tokens,
            DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS
        );
        assert_eq!(config.agent.max_wall_clock_secs, None);
        assert_eq!(
            config.api_defaults.web_search_num_results,
            DEFAULT_API_WEB_SEARCH_RESULTS
        );
        assert_eq!(
            config.api_defaults.diagnostics_timeout_ms,
            DEFAULT_API_DIAGNOSTICS_TIMEOUT_MS
        );
        assert_eq!(
            config.runtime_defaults.background_runner_poll_interval_ms,
            DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS
        );
        assert_eq!(
            config
                .runtime_defaults
                .background_runner_max_concurrent_tasks,
            DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS
        );
        assert_eq!(
            config.runtime_defaults.chat_max_session_history,
            DEFAULT_CHAT_MAX_SESSION_HISTORY
        );
        assert_eq!(
            config.channel_defaults.telegram_api_timeout_secs,
            DEFAULT_TELEGRAM_API_TIMEOUT_SECS
        );
        assert_eq!(
            config.channel_defaults.telegram_polling_timeout_secs,
            DEFAULT_TELEGRAM_POLLING_TIMEOUT_SECS
        );
        assert_eq!(
            config.registry_defaults.github_cache_ttl_secs,
            DEFAULT_GITHUB_CACHE_TTL_SECS
        );
        assert_eq!(
            config.registry_defaults.marketplace_cache_ttl_secs,
            DEFAULT_MARKETPLACE_CACHE_TTL_SECS
        );
    }

    #[test]
    fn test_update_config() {
        let ctx = setup_test_storage();

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

        ctx.storage.update_config(new_config).unwrap();

        let retrieved = ctx.storage.get_config().unwrap().unwrap();
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
        let ctx = setup_test_storage();

        let invalid_config = SystemConfig {
            worker_count: 0,
            ..Default::default()
        };

        let result = ctx.storage.update_config(invalid_config);
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
        let ctx = setup_test_storage();

        let mut config = ctx.storage.get_config().unwrap().unwrap();
        assert_eq!(
            config.agent.tool_timeout_secs,
            DEFAULT_AGENT_TOOL_TIMEOUT_SECS
        );
        assert_eq!(
            config.agent.llm_timeout_secs,
            Some(DEFAULT_AGENT_LLM_TIMEOUT_SECS)
        );
        assert_eq!(
            config.agent.bash_timeout_secs,
            DEFAULT_AGENT_BASH_TIMEOUT_SECS
        );
        assert_eq!(config.agent.max_iterations, DEFAULT_AGENT_MAX_ITERATIONS);
        assert_eq!(
            config.agent.max_parallel_subagents,
            DEFAULT_MAX_PARALLEL_SUBAGENTS
        );
        assert_eq!(
            config.agent.browser_timeout_secs,
            DEFAULT_AGENT_BROWSER_TIMEOUT_SECS
        );
        assert_eq!(
            config.agent.process_session_ttl_secs,
            DEFAULT_PROCESS_SESSION_TTL_SECS
        );
        assert_eq!(
            config.agent.approval_timeout_secs,
            DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS
        );

        config.agent.tool_timeout_secs = 180;
        config.agent.llm_timeout_secs = Some(900);
        config.agent.bash_timeout_secs = 600;
        config.agent.max_wall_clock_secs = Some(3_600);
        config.agent.max_parallel_subagents = 25;
        config.agent.browser_timeout_secs = 180;
        config.agent.process_session_ttl_secs = 7_200;
        config.agent.approval_timeout_secs = 450;
        ctx.storage.update_config(config).unwrap();

        let retrieved = ctx.storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.agent.tool_timeout_secs, 180);
        assert_eq!(retrieved.agent.llm_timeout_secs, Some(900));
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
        config.agent.llm_timeout_secs = Some(5);
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.llm_timeout_secs = None;
        assert!(config.validate().is_ok());

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
        config.agent.max_tool_concurrency = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.max_tool_result_length = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.prune_tool_max_chars = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.agent.compact_preserve_tokens = 0;
        assert!(config.validate().is_err());

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
        let ctx = setup_test_storage();
        let effective = ctx.storage.get_effective_config().unwrap();
        let stored = ctx.storage.get_config().unwrap().unwrap();
        assert_eq!(effective.worker_count, stored.worker_count);
    }

    #[test]
    fn test_effective_config_with_global_override() {
        let ctx = setup_test_storage();
        let file = write_override_file("worker_count = 42\nbackground_task_retention_days = 10");
        let _guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(effective.worker_count, 42);
        assert_eq!(effective.background_task_retention_days, 10);
    }

    #[test]
    fn test_workspace_override_precedence() {
        let ctx = setup_test_storage();
        let global_file = write_override_file("worker_count = 5\nmax_retries = 2");
        let workspace_file = write_override_file("worker_count = 9\nmax_retries = 4");
        let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, global_file.path());
        let _workspace_guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, workspace_file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(effective.worker_count, 9);
        assert_eq!(effective.max_retries, 4);
    }

    #[test]
    fn test_partial_agent_override() {
        let ctx = setup_test_storage();
        let file = write_override_file(
            r#"task_timeout_seconds = 9999

[agent]
python_timeout_secs = 45
llm_timeout_secs = 660
browser_timeout_secs = 240
process_session_ttl_secs = 5400
approval_timeout_secs = 420
max_wall_clock_secs = 7200
fallback_models = ["alpha", "beta"]
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(effective.task_timeout_seconds, 9999);
        assert_eq!(effective.agent.python_timeout_secs, 45);
        assert_eq!(effective.agent.llm_timeout_secs, Some(660));
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
    fn test_partial_api_defaults_override() {
        let ctx = setup_test_storage();
        let file = write_override_file(
            r#"[api_defaults]
web_search_num_results = 7
diagnostics_timeout_ms = 9000
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(effective.api_defaults.web_search_num_results, 7);
        assert_eq!(effective.api_defaults.diagnostics_timeout_ms, 9000);
    }

    #[test]
    fn test_partial_runtime_channel_and_registry_override() {
        let ctx = setup_test_storage();
        let file = write_override_file(
            r#"[runtime_defaults]
background_runner_poll_interval_ms = 15000
background_runner_max_concurrent_tasks = 8
chat_max_session_history = 42

[channel_defaults]
telegram_api_timeout_secs = 45
telegram_polling_timeout_secs = 55

[registry_defaults]
github_cache_ttl_secs = 900
marketplace_cache_ttl_secs = 450
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(
            effective
                .runtime_defaults
                .background_runner_poll_interval_ms,
            15000
        );
        assert_eq!(
            effective
                .runtime_defaults
                .background_runner_max_concurrent_tasks,
            8
        );
        assert_eq!(effective.runtime_defaults.chat_max_session_history, 42);
        assert_eq!(effective.channel_defaults.telegram_api_timeout_secs, 45);
        assert_eq!(effective.channel_defaults.telegram_polling_timeout_secs, 55);
        assert_eq!(effective.registry_defaults.github_cache_ttl_secs, 900);
        assert_eq!(effective.registry_defaults.marketplace_cache_ttl_secs, 450);
    }

    #[test]
    fn test_partial_agent_override_can_clear_optional_timeout() {
        let ctx = setup_test_storage();
        let file = write_override_file(
            r#"[agent]
llm_timeout_secs = "none"
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(effective.agent.llm_timeout_secs, None);
    }

    #[test]
    fn test_load_cli_config_migrates_legacy_cli_toml() {
        let _env_guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let home_dir = temp_dir.path().join("home");
        let global_config = temp_dir.path().join("config.toml");
        fs::create_dir_all(&home_dir).unwrap();
        let _home_guard = EnvGuard::set_path("HOME", &home_dir);
        let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, &global_config);

        let legacy_path = dirs::config_dir()
            .unwrap()
            .join(LEGACY_CONFIG_TOML_DIR_NAME)
            .join(CONFIG_FILE_NAME);
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(
            &legacy_path,
            r#"[default]
agent = "legacy-agent"
model = "legacy-model"
"#,
        )
        .unwrap();

        let cli = load_cli_config().unwrap();
        assert_eq!(cli.default.agent.as_deref(), Some("legacy-agent"));
        assert_eq!(cli.default.model.as_deref(), Some("legacy-model"));

        let written = fs::read_to_string(&global_config).unwrap();
        assert!(written.contains("[cli.default]"));
        assert!(written.contains("agent = \"legacy-agent\""));
        assert!(written.contains("model = \"legacy-model\""));
    }

    #[test]
    fn test_config_storage_ignores_legacy_db_config() {
        let _env_guard = env_lock();
        let temp_dir = tempdir().unwrap();
        let global_config = temp_dir.path().join("config.toml");
        let db_path = temp_dir.path().join("test.db");
        let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, &global_config);

        let db = Arc::new(Database::create(&db_path).unwrap());
        let storage = ConfigStorage::new(db).unwrap();
        let effective = storage.get_effective_config().unwrap();
        assert_eq!(effective.worker_count, DEFAULT_WORKER_COUNT);
        assert_eq!(effective.agent.max_iterations, DEFAULT_AGENT_MAX_ITERATIONS);
        assert!(!global_config.exists());
    }

    #[test]
    fn test_effective_config_sources_reports_paths_and_existence() {
        let _env_guard = env_lock();
        let global_file = write_override_file("worker_count = 7");
        let workspace_file = write_override_file("worker_count = 9");
        let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, global_file.path());
        let _workspace_guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, workspace_file.path());

        let sources = effective_config_sources().unwrap();
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
        let ctx = setup_test_storage();
        let file = write_override_file("worker_count = 0");
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let result = ctx.storage.get_effective_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_override_field_rejected() {
        let ctx = setup_test_storage();
        let file = write_override_file(
            r#"[api_defaults]
unknown_limit = 1
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        ctx.storage
            .get_effective_config()
            .expect_err("unknown override field should fail");
    }

    #[test]
    fn test_api_defaults_round_trip() {
        let ctx = setup_test_storage();
        let mut config = ctx.storage.get_config().unwrap().unwrap();
        assert_eq!(config.api_defaults.memory_search_limit, 10);
        assert_eq!(config.api_defaults.background_trace_line_limit, 200);

        config.api_defaults.memory_search_limit = 25;
        config.api_defaults.background_trace_line_limit = 300;
        ctx.storage.update_config(config).unwrap();

        let retrieved = ctx.storage.get_config().unwrap().unwrap();
        assert_eq!(retrieved.api_defaults.memory_search_limit, 25);
        assert_eq!(retrieved.api_defaults.background_trace_line_limit, 300);
    }

    #[test]
    fn test_invalid_api_defaults_rejected() {
        let mut config = SystemConfig::default();
        config.api_defaults.background_message_list_limit = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.api_defaults.web_search_num_results = MAX_API_WEB_SEARCH_RESULTS + 1;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.api_defaults.diagnostics_timeout_ms = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_runtime_channel_and_registry_defaults_rejected() {
        let mut config = SystemConfig::default();
        config.runtime_defaults.background_runner_poll_interval_ms = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config
            .runtime_defaults
            .background_runner_max_concurrent_tasks = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.runtime_defaults.chat_max_session_history = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.channel_defaults.telegram_api_timeout_secs = 5;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.channel_defaults.telegram_polling_timeout_secs = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.registry_defaults.github_cache_ttl_secs = 0;
        assert!(config.validate().is_err());

        let mut config = SystemConfig::default();
        config.registry_defaults.marketplace_cache_ttl_secs = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_api_defaults_override_from_file() {
        let ctx = setup_test_storage();
        let file = write_override_file(
            r#"[api_defaults]
memory_search_limit = 33
background_trace_line_limit = 444
"#,
        );
        let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

        let effective = ctx.storage.get_effective_config().unwrap();
        assert_eq!(effective.api_defaults.memory_search_limit, 33);
        assert_eq!(effective.api_defaults.background_trace_line_limit, 444);
    }
}
