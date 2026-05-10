//! Configuration data types shared across crates.
//!
//! Pure data structures with no database or file I/O dependencies.
//! Validation logic and TOML persistence live in `runtime`.

use serde::{Deserialize, Serialize};

use crate::defaults::*;

// ── Local constants ──────────────────────────────────────────────────

const DEFAULT_WORKER_COUNT: usize = 4;
const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 600;
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_CHAT_SESSION_RETENTION_DAYS: u32 = 30;
const DEFAULT_LOG_FILE_RETENTION_DAYS: u32 = 30;
const DEFAULT_SESSION_LIST_LIMIT: u32 = 20;

// ── CLI types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct CliConfig {
    pub version: u32,
    pub agent: Option<String>,
    pub model: Option<String>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            version: 1,
            agent: None,
            model: None,
        }
    }
}

// ── SystemSection ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct SystemSection {
    pub worker_count: usize,
    pub stall_timeout_seconds: u64,
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
}

impl Default for SystemSection {
    fn default() -> Self {
        Self {
            worker_count: DEFAULT_WORKER_COUNT,
            stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
            chat_response_timeout_seconds: None,
            max_retries: DEFAULT_MAX_RETRIES,
            chat_session_retention_days: DEFAULT_CHAT_SESSION_RETENTION_DAYS,
            log_file_retention_days: DEFAULT_LOG_FILE_RETENTION_DAYS,
            experimental_features: Vec::new(),
        }
    }
}

impl From<&SystemConfig> for SystemSection {
    fn from(config: &SystemConfig) -> Self {
        Self {
            worker_count: config.worker_count,
            stall_timeout_seconds: config.stall_timeout_seconds,
            chat_response_timeout_seconds: config.chat_response_timeout_seconds,
            max_retries: config.max_retries,
            chat_session_retention_days: config.chat_session_retention_days,
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
            fallback_models: None,
        }
    }
}

// ── ApiDefaults ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct ApiDefaults {
    pub session_list_limit: u32,
    pub web_search_num_results: usize,
}

pub type ApiSettings = ApiDefaults;

impl Default for ApiDefaults {
    fn default() -> Self {
        Self {
            session_list_limit: DEFAULT_SESSION_LIST_LIMIT,
            web_search_num_results: DEFAULT_API_WEB_SEARCH_RESULTS,
        }
    }
}

// ── RuntimeDefaults ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct RuntimeDefaults {
    pub chat_max_session_history: usize,
}

pub type RuntimeSettings = RuntimeDefaults;

impl Default for RuntimeDefaults {
    fn default() -> Self {
        Self {
            chat_max_session_history: DEFAULT_CHAT_MAX_SESSION_HISTORY,
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
    pub stall_timeout_seconds: u64,
    #[serde(default)]
    pub chat_response_timeout_seconds: Option<u64>,
    pub max_retries: u32,
    pub chat_session_retention_days: u32,
    pub log_file_retention_days: u32,
    pub experimental_features: Vec<String>,
    #[serde(default)]
    pub agent: AgentSettings,
    #[serde(default)]
    pub api_defaults: ApiSettings,
    #[serde(default)]
    pub runtime_defaults: RuntimeSettings,
    #[serde(default)]
    pub registry_defaults: RegistrySettings,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            worker_count: DEFAULT_WORKER_COUNT,
            stall_timeout_seconds: DEFAULT_STALL_TIMEOUT_SECONDS,
            chat_response_timeout_seconds: None,
            max_retries: DEFAULT_MAX_RETRIES,
            chat_session_retention_days: DEFAULT_CHAT_SESSION_RETENTION_DAYS,
            log_file_retention_days: DEFAULT_LOG_FILE_RETENTION_DAYS,
            experimental_features: Vec::new(),
            agent: AgentSettings::default(),
            api_defaults: ApiSettings::default(),
            runtime_defaults: RuntimeSettings::default(),
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
            registry: system.registry_defaults,
            cli,
        }
    }

    pub fn system_config(&self) -> SystemConfig {
        SystemConfig {
            worker_count: self.system.worker_count,
            stall_timeout_seconds: self.system.stall_timeout_seconds,
            chat_response_timeout_seconds: self.system.chat_response_timeout_seconds,
            max_retries: self.system.max_retries,
            chat_session_retention_days: self.system.chat_session_retention_days,
            log_file_retention_days: self.system.log_file_retention_days,
            experimental_features: self.system.experimental_features.clone(),
            agent: self.agent.clone(),
            api_defaults: self.api.clone(),
            runtime_defaults: self.runtime.clone(),
            registry_defaults: self.registry.clone(),
        }
    }

    pub fn replace_system_config(&mut self, system: SystemConfig) {
        self.system = SystemSection::from(&system);
        self.agent = system.agent;
        self.api = system.api_defaults;
        self.runtime = system.runtime_defaults;
        self.registry = system.registry_defaults;
    }
}
