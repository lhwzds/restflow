//! Configuration data types shared across crates.
//!
//! Pure data structures with no database or file I/O dependencies.
//! Validation logic and TOML persistence remain in `restflow-storage`.

use serde::{Deserialize, Serialize};

use crate::defaults::*;

// ── Local constants ──────────────────────────────────────────────────

const DEFAULT_WORKER_COUNT: usize = 4;
const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 1800;
const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 600;
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_CHAT_SESSION_RETENTION_DAYS: u32 = 30;
const DEFAULT_BACKGROUND_TASK_RETENTION_DAYS: u32 = 7;
const DEFAULT_CHECKPOINT_RETENTION_DAYS: u32 = 3;
const DEFAULT_MEMORY_CHUNK_RETENTION_DAYS: u32 = 90;
const DEFAULT_LOG_FILE_RETENTION_DAYS: u32 = 30;
const DEFAULT_MEMORY_SEARCH_LIMIT: u32 = 10;
const DEFAULT_SESSION_LIST_LIMIT: u32 = 20;

fn default_cli_timeout() -> u64 {
    120
}

fn default_cli_max_output() -> usize {
    1_048_576
}

// ── CLI types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct CliConfig {
    pub version: u32,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub sandbox: CliSandboxConfig,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            version: 1,
            agent: None,
            model: None,
            sandbox: CliSandboxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct CliSandboxConfig {
    pub enabled: bool,
    pub env: CliEnvSandboxConfig,
    pub limits: CliLimitsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct CliEnvSandboxConfig {
    pub isolate: bool,
    pub allow: Vec<String>,
    pub block: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
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

// ── SystemSection ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct SystemSection {
    pub worker_count: usize,
    pub task_timeout_seconds: u64,
    pub stall_timeout_seconds: u64,
    #[serde(default)]
    pub background_api_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub background_task_retention_days: u32,
    pub checkpoint_retention_days: u32,
    pub memory_chunk_retention_days: u32,
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
}

impl Default for SystemSection {
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
        }
    }
}

impl From<&SystemConfig> for SystemSection {
    fn from(config: &SystemConfig) -> Self {
        Self {
            worker_count: config.worker_count,
            task_timeout_seconds: config.task_timeout_seconds,
            stall_timeout_seconds: config.stall_timeout_seconds,
            background_api_timeout_seconds: config.background_api_timeout_seconds,
            chat_response_timeout_seconds: config.chat_response_timeout_seconds,
            max_retries: config.max_retries,
            chat_session_retention_days: config.chat_session_retention_days,
            background_task_retention_days: config.background_task_retention_days,
            checkpoint_retention_days: config.checkpoint_retention_days,
            memory_chunk_retention_days: config.memory_chunk_retention_days,
            log_file_retention_days: config.log_file_retention_days,
            experimental_features: config.experimental_features.clone(),
        }
    }
}

// ── AgentDefaults ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct AgentDefaults {
    pub tool_timeout_secs: u64,
    pub llm_timeout_secs: Option<u64>,
    pub bash_timeout_secs: u64,
    pub python_timeout_secs: u64,
    pub browser_timeout_secs: u64,
    pub process_session_ttl_secs: u64,
    pub approval_timeout_secs: u64,
    pub max_iterations: usize,
    pub max_depth: usize,
    pub subagent_timeout_secs: u64,
    pub max_parallel_subagents: usize,
    pub max_tool_calls: usize,
    pub max_tool_concurrency: usize,
    pub max_tool_result_length: usize,
    pub prune_tool_max_chars: usize,
    pub compact_preserve_tokens: usize,
    pub max_wall_clock_secs: Option<u64>,
    pub default_task_timeout_secs: u64,
    pub default_max_duration_secs: u64,
    #[serde(default)]
    pub fallback_models: Option<Vec<String>>,
}

pub type AgentSettings = AgentDefaults;

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
            max_depth: DEFAULT_SUBAGENT_MAX_DEPTH,
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

// ── ApiDefaults ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct ApiDefaults {
    pub memory_search_limit: u32,
    pub session_list_limit: u32,
    pub background_progress_event_limit: usize,
    pub background_message_list_limit: usize,
    pub background_trace_list_limit: usize,
    pub background_trace_line_limit: usize,
    pub web_search_num_results: usize,
    pub diagnostics_timeout_ms: u64,
}

pub type ApiSettings = ApiDefaults;

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

// ── RuntimeDefaults ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct RuntimeDefaults {
    pub background_runner_poll_interval_ms: u64,
    pub background_runner_max_concurrent_tasks: usize,
    pub chat_max_session_history: usize,
}

pub type RuntimeSettings = RuntimeDefaults;

impl Default for RuntimeDefaults {
    fn default() -> Self {
        Self {
            background_runner_poll_interval_ms: DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS,
            background_runner_max_concurrent_tasks: DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS,
            chat_max_session_history: DEFAULT_CHAT_MAX_SESSION_HISTORY,
        }
    }
}

// ── ChannelDefaults ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct ChannelDefaults {
    pub telegram_api_timeout_secs: u64,
    pub telegram_polling_timeout_secs: u32,
}

pub type ChannelSettings = ChannelDefaults;

impl Default for ChannelDefaults {
    fn default() -> Self {
        Self {
            telegram_api_timeout_secs: DEFAULT_TELEGRAM_API_TIMEOUT_SECS,
            telegram_polling_timeout_secs: DEFAULT_TELEGRAM_POLLING_TIMEOUT_SECS,
        }
    }
}

// ── RegistryDefaults ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct RegistryDefaults {
    pub github_cache_ttl_secs: u64,
    pub marketplace_cache_ttl_secs: u64,
}

pub type RegistrySettings = RegistryDefaults;

impl Default for RegistryDefaults {
    fn default() -> Self {
        Self {
            github_cache_ttl_secs: DEFAULT_GITHUB_CACHE_TTL_SECS,
            marketplace_cache_ttl_secs: DEFAULT_MARKETPLACE_CACHE_TTL_SECS,
        }
    }
}

// ── SystemConfig ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct SystemConfig {
    pub worker_count: usize,
    pub task_timeout_seconds: u64,
    pub stall_timeout_seconds: u64,
    #[serde(default)]
    pub background_api_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub background_task_retention_days: u32,
    pub checkpoint_retention_days: u32,
    pub memory_chunk_retention_days: u32,
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
    #[serde(default)]
    pub agent: AgentSettings,
    #[serde(default)]
    pub api_defaults: ApiSettings,
    #[serde(default)]
    pub runtime_defaults: RuntimeSettings,
    #[serde(default)]
    pub channel_defaults: ChannelSettings,
    #[serde(default)]
    pub registry_defaults: RegistrySettings,
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
            agent: AgentSettings::default(),
            api_defaults: ApiSettings::default(),
            runtime_defaults: RuntimeSettings::default(),
            channel_defaults: ChannelSettings::default(),
            registry_defaults: RegistrySettings::default(),
        }
    }
}

// ── ConfigDocument ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub struct ConfigDocument {
    pub system: SystemSection,
    pub agent: AgentSettings,
    pub api: ApiSettings,
    pub runtime: RuntimeSettings,
    pub channel: ChannelSettings,
    pub registry: RegistrySettings,
    #[serde(default)]
    pub cli: CliConfig,
}

impl ConfigDocument {
    pub fn from_system_config(system: SystemConfig, cli: CliConfig) -> Self {
        Self {
            system: SystemSection::from(&system),
            agent: system.agent,
            api: system.api_defaults,
            runtime: system.runtime_defaults,
            channel: system.channel_defaults,
            registry: system.registry_defaults,
            cli,
        }
    }

    pub fn system_config(&self) -> SystemConfig {
        SystemConfig {
            worker_count: self.system.worker_count,
            task_timeout_seconds: self.system.task_timeout_seconds,
            stall_timeout_seconds: self.system.stall_timeout_seconds,
            background_api_timeout_seconds: self.system.background_api_timeout_seconds,
            chat_response_timeout_seconds: self.system.chat_response_timeout_seconds,
            max_retries: self.system.max_retries,
            chat_session_retention_days: self.system.chat_session_retention_days,
            background_task_retention_days: self.system.background_task_retention_days,
            checkpoint_retention_days: self.system.checkpoint_retention_days,
            memory_chunk_retention_days: self.system.memory_chunk_retention_days,
            log_file_retention_days: self.system.log_file_retention_days,
            experimental_features: self.system.experimental_features.clone(),
            agent: self.agent.clone(),
            api_defaults: self.api.clone(),
            runtime_defaults: self.runtime.clone(),
            channel_defaults: self.channel.clone(),
            registry_defaults: self.registry.clone(),
        }
    }

    pub fn replace_system_config(&mut self, system: SystemConfig) {
        self.system = SystemSection::from(&system);
        self.agent = system.agent;
        self.api = system.api_defaults;
        self.runtime = system.runtime_defaults;
        self.channel = system.channel_defaults;
        self.registry = system.registry_defaults;
    }
}
