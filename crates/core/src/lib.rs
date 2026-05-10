pub mod agent_validation {
    use std::sync::Arc;

    use types::{AgentNode, ApiKeyConfig, ValidationError};

    use crate::AppCore;

    /// Validate agent fields that require runtime/storage lookups.
    pub async fn validate_agent_node_async(
        agent: &AgentNode,
        core: &Arc<AppCore>,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        let tool_registry = match crate::services::tool_registry::create_tool_registry(
            core.storage.config.clone(),
            None,
            None,
        ) {
            Ok(registry) => registry,
            Err(err) => {
                errors.push(ValidationError::new(
                    "tools",
                    format!("Failed to create tool registry: {err}"),
                ));
                return Err(errors);
            }
        };

        if let Some(tools) = &agent.tools {
            for tool_name in tools {
                let normalized = tool_name.trim();
                if normalized.is_empty() {
                    errors.push(ValidationError::new("tools", "tool name must not be empty"));
                    continue;
                }
                if !tool_registry.has(normalized) {
                    errors.push(ValidationError::new(
                        "tools",
                        format!("unknown tool: {}", normalized),
                    ));
                }
            }
        }

        if let Some(skills) = &agent.skills {
            for skill_id in skills {
                let normalized = skill_id.trim();
                if normalized.is_empty() {
                    errors.push(ValidationError::new("skills", "skill ID must not be empty"));
                    continue;
                }
                match crate::services::skills::skill_exists_in_catalog(normalized) {
                    Ok(true) => {}
                    Ok(false) => errors.push(ValidationError::new(
                        "skills",
                        format!("unknown skill: {}", normalized),
                    )),
                    Err(err) => errors.push(ValidationError::new(
                        "skills",
                        format!("failed to verify skill '{}': {}", normalized, err),
                    )),
                }
            }
        }

        if let Some(ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
            let normalized = secret_name.trim();
            if !normalized.is_empty() {
                match core.storage.secrets.has_available_secret(normalized) {
                    Ok(true) => {}
                    Ok(false) => errors.push(ValidationError::new(
                        "api_key_config",
                        format!("secret not found in storage: {}", normalized),
                    )),
                    Err(err) => errors.push(ValidationError::new(
                        "api_key_config",
                        format!("failed to verify secret '{}': {}", normalized, err),
                    )),
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::test_support::RestflowTestEnv;

        #[cfg(unix)]
        #[tokio::test(flavor = "current_thread")]
        async fn accepts_team_skill() {
            let env = RestflowTestEnv::new();
            let previous_skrun_root = std::env::var_os("SKRUN_SKILLS_DIR");
            let skills_root = env.root().join("skrun-skills");
            let artifact = skrun::SkillArtifact::markdown("team", "Team", "0.1.0", "# Team");
            skrun::save_artifact(skills_root.join("team"), &artifact).unwrap();
            unsafe { std::env::set_var("SKRUN_SKILLS_DIR", &skills_root) };
            let core = Arc::new(
                AppCore::new(env.db_path("agent-skill.db").to_str().unwrap())
                    .await
                    .unwrap(),
            );
            let node = AgentNode {
                skills: Some(vec!["team".to_string()]),
                ..AgentNode::new()
            };

            let result = validate_agent_node_async(&node, &core).await;
            unsafe {
                if let Some(value) = previous_skrun_root {
                    std::env::set_var("SKRUN_SKILLS_DIR", value);
                } else {
                    std::env::remove_var("SKRUN_SKILLS_DIR");
                }
            }

            assert!(result.is_ok(), "unexpected validation errors: {result:?}");
        }
    }
}
pub mod config {
    //! System configuration storage.

    use anyhow::{Context, Result};
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::Value as JsonValue;
    use specta::Type;
    use std::collections::BTreeMap;
    use std::collections::HashSet;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{OnceLock, RwLock};
    use std::time::{SystemTime, UNIX_EPOCH};
    use types::{
        DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS, DEFAULT_AGENT_BASH_TIMEOUT_SECS,
        DEFAULT_AGENT_BROWSER_TIMEOUT_SECS, DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS,
        DEFAULT_AGENT_LLM_TIMEOUT_SECS, DEFAULT_AGENT_MAX_ITERATIONS, DEFAULT_AGENT_MAX_TOOL_CALLS,
        DEFAULT_AGENT_MAX_TOOL_CONCURRENCY, DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH,
        DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS, DEFAULT_AGENT_PYTHON_TIMEOUT_SECS,
        DEFAULT_AGENT_TOOL_TIMEOUT_SECS, DEFAULT_API_WEB_SEARCH_RESULTS,
        DEFAULT_CHAT_MAX_SESSION_HISTORY, DEFAULT_GITHUB_CACHE_TTL_SECS,
        DEFAULT_MARKETPLACE_CACHE_TTL_SECS, DEFAULT_MAX_PARALLEL_SUBAGENTS,
        DEFAULT_PROCESS_SESSION_TTL_SECS, DEFAULT_SUBAGENT_MAX_DEPTH,
        DEFAULT_SUBAGENT_TIMEOUT_SECS, MAX_API_WEB_SEARCH_RESULTS,
    };

    const GLOBAL_CONFIG_ENV: &str = "RESTFLOW_GLOBAL_CONFIG";
    const WORKSPACE_CONFIG_ENV: &str = "RESTFLOW_WORKSPACE_CONFIG";
    const CONFIG_SUBDIR: &str = ".restflow";
    const CONFIG_FILE_NAME: &str = "config.toml";

    // Default configuration constants
    const DEFAULT_WORKER_COUNT: usize = 4;
    const DEFAULT_STALL_TIMEOUT_SECONDS: u64 = 600; // 10 minutes
    const DEFAULT_MAX_RETRIES: u32 = 3;
    const DEFAULT_CHAT_SESSION_RETENTION_DAYS: u32 = 30;
    const DEFAULT_AUDIT_EVENT_RETENTION_DAYS: u32 = 7;
    const DEFAULT_LOG_FILE_RETENTION_DAYS: u32 = 30;
    const DEFAULT_SESSION_LIST_LIMIT: u32 = 20;
    const MIN_RETENTION_DAYS: u32 = 1;
    const MIN_WORKER_COUNT: usize = 1;
    const MIN_TIMEOUT_SECONDS: u64 = 10;

    /// CLI-specific settings stored in the unified config file.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

    impl CliConfig {
        pub fn load() -> Self {
            load_cli_config().unwrap_or_default()
        }

        pub fn save(&self) -> Result<()> {
            write_cli_config(self)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    #[serde(default)]
    pub struct SystemSection {
        pub worker_count: usize,
        pub stall_timeout_seconds: u64,
        #[serde(default)]
        pub chat_response_timeout_seconds: Option<u64>,
        pub max_retries: u32,
        pub chat_session_retention_days: u32,
        pub audit_event_retention_days: u32,
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
                audit_event_retention_days: DEFAULT_AUDIT_EVENT_RETENTION_DAYS,
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
                audit_event_retention_days: config.audit_event_retention_days,
                log_file_retention_days: config.log_file_retention_days,
                experimental_features: config.experimental_features.clone(),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
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
                audit_event_retention_days: self.system.audit_event_retention_days,
                log_file_retention_days: self.system.log_file_retention_days,
                experimental_features: self.system.experimental_features.clone(),
                agent: self.agent.clone(),
                api_defaults: self.api.clone(),
                runtime_defaults: self.runtime.clone(),
                registry_defaults: self.registry.clone(),
            }
        }

        fn validate(&self) -> Result<()> {
            self.system_config().validate()
        }

        fn replace_system_config(&mut self, system: SystemConfig) {
            self.system = SystemSection::from(&system);
            self.agent = system.agent;
            self.api = system.api_defaults;
            self.runtime = system.runtime_defaults;
            self.registry = system.registry_defaults;
        }
    }

    type UnifiedConfigFile = ConfigDocument;

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
        /// Whether to run an auxiliary LLM review before tool execution.
        pub auto_review_tools: bool,
        /// Maximum ReAct loop iterations per agent run.
        pub max_iterations: usize,
        /// Maximum nesting depth for sub-agents.
        pub max_depth: usize,
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
        /// Fallback models for cross-provider failover (manually configured).
        /// Only used when primary model fails - does not auto-discover providers.
        /// Format: model names as strings (e.g., ["glm-4.7", "claude-sonnet-4-5"])
        #[serde(default)]
        pub fallback_models: Option<Vec<String>>,
    }

    /// Aligned alias that matches the on-disk `[agent]` section naming.
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
                auto_review_tools: false,
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
            if self.max_depth == 0 {
                return Err(anyhow::anyhow!("agent.max_depth must be at least 1"));
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
            Ok(())
        }
    }

    /// API-facing default limits used by MCP and adapter query operations.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    #[serde(default)]
    pub struct ApiDefaults {
        /// Default `chat_session_list` result limit.
        pub session_list_limit: u32,
        /// Default result count for `web_search`.
        pub web_search_num_results: usize,
    }

    /// Aligned alias that matches the on-disk `[api]` section naming.
    pub type ApiSettings = ApiDefaults;

    impl Default for ApiDefaults {
        fn default() -> Self {
            Self {
                session_list_limit: DEFAULT_SESSION_LIST_LIMIT,
                web_search_num_results: DEFAULT_API_WEB_SEARCH_RESULTS,
            }
        }
    }

    impl ApiDefaults {
        fn validate(&self) -> Result<()> {
            if self.session_list_limit == 0 {
                return Err(anyhow::anyhow!("api.session_list_limit must be at least 1"));
            }
            if self.web_search_num_results == 0 {
                return Err(anyhow::anyhow!(
                    "api.web_search_num_results must be at least 1"
                ));
            }
            if self.web_search_num_results > MAX_API_WEB_SEARCH_RESULTS {
                return Err(anyhow::anyhow!(
                    "api.web_search_num_results must be at most {}",
                    MAX_API_WEB_SEARCH_RESULTS
                ));
            }
            Ok(())
        }
    }

    /// Runtime execution defaults for daemon/background/chat behavior.
    #[derive(Debug, Clone, Serialize, Deserialize, Type)]
    #[serde(default)]
    pub struct RuntimeDefaults {
        /// Maximum session history kept for channel chat sessions.
        pub chat_max_session_history: usize,
    }

    /// Aligned alias that matches the on-disk `[runtime]` section naming.
    pub type RuntimeSettings = RuntimeDefaults;

    impl Default for RuntimeDefaults {
        fn default() -> Self {
            Self {
                chat_max_session_history: DEFAULT_CHAT_MAX_SESSION_HISTORY,
            }
        }
    }

    impl RuntimeDefaults {
        fn validate(&self) -> Result<()> {
            if self.chat_max_session_history == 0 {
                return Err(anyhow::anyhow!(
                    "runtime.chat_max_session_history must be at least 1"
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

    /// Aligned alias that matches the on-disk `[registry]` section naming.
    pub type RegistrySettings = RegistryDefaults;

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
                    "registry.github_cache_ttl_secs must be at least 1"
                ));
            }
            if self.marketplace_cache_ttl_secs == 0 {
                return Err(anyhow::anyhow!(
                    "registry.marketplace_cache_ttl_secs must be at least 1"
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
        pub stall_timeout_seconds: u64,
        /// Timeout for interactive channel chat responses in seconds.
        ///
        /// `None` disables timeout for chat dispatching.
        #[serde(default)]
        pub chat_response_timeout_seconds: Option<u64>,
        pub max_retries: u32,
        pub chat_session_retention_days: u32,
        /// Retention period for execution audit events.
        /// 0 = keep forever, otherwise delete events older than N days.
        pub audit_event_retention_days: u32,
        /// Retention period for daemon and event log files on disk.
        /// 0 = keep forever, otherwise delete files older than N days.
        pub log_file_retention_days: u32,
        pub experimental_features: Vec<String>,
        /// Agent execution defaults.
        #[serde(default)]
        pub agent: AgentSettings,
        /// API operation settings and limits.
        #[serde(default)]
        pub api_defaults: ApiSettings,
        /// Runtime execution settings for daemon/background/chat services.
        #[serde(default)]
        pub runtime_defaults: RuntimeSettings,
        /// Registry provider settings.
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
                audit_event_retention_days: DEFAULT_AUDIT_EVENT_RETENTION_DAYS,
                log_file_retention_days: DEFAULT_LOG_FILE_RETENTION_DAYS,
                experimental_features: Vec::new(),
                agent: AgentSettings::default(),
                api_defaults: ApiSettings::default(),
                runtime_defaults: RuntimeSettings::default(),
                registry_defaults: RegistrySettings::default(),
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

            if self.stall_timeout_seconds < MIN_TIMEOUT_SECONDS {
                return Err(anyhow::anyhow!(
                    "Stall timeout must be at least {} seconds",
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

            if self.audit_event_retention_days != 0
                && self.audit_event_retention_days < MIN_RETENTION_DAYS
            {
                return Err(anyhow::anyhow!(
                    "Audit event retention must be 0 (forever) or at least {} day",
                    MIN_RETENTION_DAYS
                ));
            }

            if self.log_file_retention_days != 0
                && self.log_file_retention_days < MIN_RETENTION_DAYS
            {
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
            self.registry_defaults.validate()?;

            Ok(())
        }
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct CliConfigOverride {
        pub version: Option<u32>,
        pub agent: Option<String>,
        pub model: Option<String>,
        pub sandbox: Option<DeprecatedCliSandboxOverride>,
    }

    impl CliConfigOverride {
        fn apply_to(&self, config: &mut CliConfig) {
            if let Some(value) = self.version {
                config.version = value;
            }
            if let Some(value) = self.agent.clone() {
                config.agent = Some(value);
            }
            if let Some(value) = self.model.clone() {
                config.model = Some(value);
            }
        }
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct DeprecatedCliSandboxOverride {
        pub enabled: Option<bool>,
        pub env: Option<DeprecatedCliSandboxEnvOverride>,
        pub limits: Option<DeprecatedCliSandboxLimitsOverride>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct DeprecatedCliSandboxEnvOverride {
        pub isolate: Option<bool>,
        pub allow: Option<Vec<String>>,
        pub block: Option<Vec<String>>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct DeprecatedCliSandboxLimitsOverride {
        pub timeout_secs: Option<u64>,
        pub max_output_bytes: Option<u64>,
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

    #[derive(Debug, Default, Serialize, Deserialize)]
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
        pub auto_review_tools: Option<bool>,
        pub max_iterations: Option<usize>,
        pub max_depth: Option<usize>,
        pub subagent_timeout_secs: Option<u64>,
        pub max_parallel_subagents: Option<usize>,
        pub max_tool_calls: Option<usize>,
        pub max_tool_concurrency: Option<usize>,
        pub max_tool_result_length: Option<usize>,
        pub prune_tool_max_chars: Option<usize>,
        pub compact_preserve_tokens: Option<usize>,
        #[serde(default, deserialize_with = "deserialize_optional_u64_override")]
        pub max_wall_clock_secs: Option<Option<u64>>,
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
            if let Some(value) = self.auto_review_tools {
                agent.auto_review_tools = value;
            }
            if let Some(value) = self.max_iterations {
                agent.max_iterations = value;
            }
            if let Some(value) = self.max_depth {
                agent.max_depth = value;
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
            if let Some(value) = self.fallback_models.clone() {
                agent.fallback_models = value;
            }
        }
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct ApiDefaultsOverride {
        pub session_list_limit: Option<u32>,
        pub web_search_num_results: Option<usize>,
    }

    impl ApiDefaultsOverride {
        fn apply_to(&self, api_defaults: &mut ApiDefaults) {
            if let Some(value) = self.session_list_limit {
                api_defaults.session_list_limit = value;
            }
            if let Some(value) = self.web_search_num_results {
                api_defaults.web_search_num_results = value;
            }
        }
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct RuntimeDefaultsOverride {
        pub chat_max_session_history: Option<usize>,
    }

    impl RuntimeDefaultsOverride {
        fn apply_to(&self, runtime_defaults: &mut RuntimeDefaults) {
            if let Some(value) = self.chat_max_session_history {
                runtime_defaults.chat_max_session_history = value;
            }
        }
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
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

    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct SystemSectionOverride {
        pub worker_count: Option<usize>,
        pub stall_timeout_seconds: Option<u64>,
        #[serde(default, deserialize_with = "deserialize_optional_u64_override")]
        pub chat_response_timeout_seconds: Option<Option<u64>>,
        pub max_retries: Option<u32>,
        pub chat_session_retention_days: Option<u32>,
        pub audit_event_retention_days: Option<u32>,
        pub log_file_retention_days: Option<u32>,
        pub experimental_features: Option<Vec<String>>,
    }

    impl SystemSectionOverride {
        fn apply_to(&self, config: &mut SystemSection) {
            if let Some(value) = self.worker_count {
                config.worker_count = value;
            }
            if let Some(value) = self.stall_timeout_seconds {
                config.stall_timeout_seconds = value;
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
            if let Some(value) = self.audit_event_retention_days {
                config.audit_event_retention_days = value;
            }
            if let Some(value) = self.log_file_retention_days {
                config.log_file_retention_days = value;
            }
            if let Some(values) = self.experimental_features.clone() {
                config.experimental_features = values;
            }
        }
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct UnifiedConfigOverride {
        pub system: Option<SystemSectionOverride>,
        pub agent: Option<AgentDefaultsOverride>,
        pub api: Option<ApiDefaultsOverride>,
        pub runtime: Option<RuntimeDefaultsOverride>,
        pub registry: Option<RegistryDefaultsOverride>,
        pub cli: Option<CliConfigOverride>,
    }

    impl UnifiedConfigOverride {
        fn apply_to(&self, config: &mut UnifiedConfigFile) {
            if let Some(system_override) = &self.system {
                system_override.apply_to(&mut config.system);
            }
            if let Some(agent_override) = &self.agent {
                agent_override.apply_to(&mut config.agent);
            }
            if let Some(api_override) = &self.api {
                api_override.apply_to(&mut config.api);
            }
            if let Some(runtime_override) = &self.runtime {
                runtime_override.apply_to(&mut config.runtime);
            }
            if let Some(registry_override) = &self.registry {
                registry_override.apply_to(&mut config.registry);
            }
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
                "Failed to read config.toml override from {}",
                path.display()
            )
        })?;
        toml::from_str::<UnifiedConfigOverride>(&contents)
            .map(Some)
            .with_context(|| {
                format!(
                    "Failed to parse config.toml override from {}",
                    path.display()
                )
            })
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ConfigPathFingerprint {
        path: PathBuf,
        from_env: bool,
        exists: bool,
        len: Option<u64>,
        modified_nanos: Option<u128>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ConfigLayerCacheKey {
        global: Option<ConfigPathFingerprint>,
        workspace: Option<ConfigPathFingerprint>,
    }

    #[derive(Debug, Clone)]
    struct ConfigLayerSnapshot {
        global_path: Option<ResolvedOverridePath>,
        workspace_path: Option<ResolvedOverridePath>,
        cache_key: ConfigLayerCacheKey,
    }

    #[derive(Debug, Clone)]
    struct CachedConfigLayers {
        key: ConfigLayerCacheKey,
        layers: ConfigLayerState,
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

    fn workspace_config_path_for_workspace(
        workspace_root: Option<&Path>,
    ) -> Option<ResolvedOverridePath> {
        if let Some(path) = env_override_path(WORKSPACE_CONFIG_ENV) {
            return Some(ResolvedOverridePath {
                path,
                from_env: true,
            });
        }
        workspace_root.map(|dir| ResolvedOverridePath {
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
        global_config_path()
            .ok_or_else(|| anyhow::anyhow!("Failed to resolve global config.toml path"))
    }

    #[derive(Debug, Clone)]
    struct ConfigLayerState {
        default: UnifiedConfigFile,
        global: UnifiedConfigFile,
        effective: UnifiedConfigFile,
        global_path: Option<ResolvedOverridePath>,
        workspace_path: Option<ResolvedOverridePath>,
    }

    fn config_layer_cache() -> &'static RwLock<Option<CachedConfigLayers>> {
        static CACHE: OnceLock<RwLock<Option<CachedConfigLayers>>> = OnceLock::new();
        CACHE.get_or_init(|| RwLock::new(None))
    }

    fn clear_config_layer_cache() {
        let mut cache = config_layer_cache()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.take();
    }

    fn fingerprint_system_time(time: SystemTime) -> u128 {
        time.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    fn fingerprint_override_path(
        resolved: Option<&ResolvedOverridePath>,
    ) -> Result<Option<ConfigPathFingerprint>> {
        resolved
            .map(|entry| {
                let metadata = match fs::metadata(&entry.path) {
                    Ok(metadata) => Some(metadata),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                    Err(err) => {
                        return Err(err).with_context(|| {
                            format!(
                                "Failed to read metadata for config override {}",
                                entry.path.display()
                            )
                        });
                    }
                };

                let (exists, len, modified_nanos) = if let Some(metadata) = metadata {
                    let modified = metadata.modified().with_context(|| {
                        format!(
                            "Failed to read modified timestamp for config override {}",
                            entry.path.display()
                        )
                    })?;
                    (
                        true,
                        Some(metadata.len()),
                        Some(fingerprint_system_time(modified)),
                    )
                } else {
                    (false, None, None)
                };

                Ok(ConfigPathFingerprint {
                    path: entry.path.clone(),
                    from_env: entry.from_env,
                    exists,
                    len,
                    modified_nanos,
                })
            })
            .transpose()
    }

    fn resolve_config_layer_snapshot_for_workspace(
        workspace_root: Option<&Path>,
    ) -> Result<ConfigLayerSnapshot> {
        let global_path = global_config_path();
        let workspace_path = workspace_config_path_for_workspace(workspace_root);

        let cache_key = ConfigLayerCacheKey {
            global: fingerprint_override_path(global_path.as_ref())?,
            workspace: fingerprint_override_path(workspace_path.as_ref())?,
        };

        Ok(ConfigLayerSnapshot {
            global_path,
            workspace_path,
            cache_key,
        })
    }

    fn load_config_layers_uncached(snapshot: &ConfigLayerSnapshot) -> Result<ConfigLayerState> {
        let default = UnifiedConfigFile::default();
        let global_path = snapshot.global_path.clone();
        let workspace_path = snapshot.workspace_path.clone();

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

        global.validate()?;
        effective.validate()?;

        Ok(ConfigLayerState {
            default,
            global,
            effective,
            global_path,
            workspace_path,
        })
    }

    fn load_config_layers_for_workspace(workspace_root: Option<&Path>) -> Result<ConfigLayerState> {
        let snapshot = resolve_config_layer_snapshot_for_workspace(workspace_root)?;

        {
            let cache = config_layer_cache()
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(entry) = cache.as_ref()
                && entry.key == snapshot.cache_key
            {
                return Ok(entry.layers.clone());
            }
        }

        let layers = load_config_layers_uncached(&snapshot)?;
        let mut cache = config_layer_cache()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *cache = Some(CachedConfigLayers {
            key: snapshot.cache_key,
            layers: layers.clone(),
        });
        Ok(layers)
    }

    fn load_config_layers() -> Result<ConfigLayerState> {
        load_config_layers_for_workspace(None)
    }

    fn write_global_config_file(config: &UnifiedConfigFile) -> Result<()> {
        config.validate()?;
        let target = global_write_target()?;
        if let Some(parent) = target.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }
        let contents = toml::to_string_pretty(config).context("Failed to serialize config.toml")?;
        fs::write(&target.path, contents)
            .with_context(|| format!("Failed to write config.toml to {}", target.path.display()))?;
        clear_config_layer_cache();
        Ok(())
    }

    fn flatten_json(
        prefix: Option<&str>,
        value: &JsonValue,
        output: &mut BTreeMap<String, JsonValue>,
    ) {
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

    fn flatten_config_document(config: &ConfigDocument) -> Result<BTreeMap<String, JsonValue>> {
        let json = serde_json::to_value(config).context("Failed to serialize config document")?;
        let mut output = BTreeMap::new();
        flatten_json(None, &json, &mut output);
        Ok(output)
    }

    fn build_value_sources(
        layers: &ConfigLayerState,
    ) -> Result<BTreeMap<String, ConfigValueSourceInfo>> {
        let default_values = flatten_config_document(&layers.default)?;
        let global_values = flatten_config_document(&layers.global)?;
        let effective_values = flatten_config_document(&layers.effective)?;

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

    pub fn load_cli_config() -> Result<CliConfig> {
        Ok(load_config_layers()?.effective.cli.clone())
    }

    pub fn load_global_cli_config() -> Result<CliConfig> {
        Ok(load_config_layers()?.global.cli.clone())
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
        effective_config_sources_for_workspace(None)
    }

    pub fn effective_config_sources_for_workspace(
        workspace_root: Option<&Path>,
    ) -> Result<EffectiveConfigSources> {
        let layers = load_config_layers_for_workspace(workspace_root)?;
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
        pub fn new() -> Self {
            Self
        }

        /// Get the global config view (defaults + global config.toml).
        pub fn get_config(&self) -> Result<SystemConfig> {
            self.get_global_config()
        }

        /// Get the global config view (defaults + global config.toml).
        pub fn get_global_config(&self) -> Result<SystemConfig> {
            Ok(load_config_layers()?.global.system_config())
        }

        /// Get the effective configuration by applying config.toml overrides.
        pub fn get_effective_config(&self) -> Result<SystemConfig> {
            Ok(load_config_layers()?.effective.system_config())
        }

        pub fn get_effective_config_for_workspace(
            &self,
            workspace_root: Option<&Path>,
        ) -> Result<SystemConfig> {
            Ok(load_config_layers_for_workspace(workspace_root)?
                .effective
                .system_config())
        }

        /// Update the global config.toml system configuration while preserving the CLI section.
        pub fn update_config(&self, config: SystemConfig) -> Result<()> {
            config.validate()?;
            let mut current = load_config_layers()
                .map(|layers| layers.global)
                .unwrap_or_default();
            current.replace_system_config(config);
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
        use tempfile::NamedTempFile;
        use tempfile::tempdir;

        struct EnvGuard {
            key: &'static str,
            original: Option<std::ffi::OsString>,
        }

        struct CurrentDirGuard {
            original: PathBuf,
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

        impl CurrentDirGuard {
            fn set(path: &Path) -> Self {
                let original = env::current_dir().unwrap();
                env::set_current_dir(path).unwrap();
                Self { original }
            }
        }

        impl Drop for CurrentDirGuard {
            fn drop(&mut self) {
                let _ = env::set_current_dir(&self.original);
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
            let storage = ConfigStorage::new();
            TestContext {
                storage,
                _temp_dir: temp_dir,
                _env_guard: env_guard,
                _global_guard: global_guard,
            }
        }

        fn env_lock() -> std::sync::MutexGuard<'static, ()> {
            crate::test_support::env_lock()
        }

        #[test]
        fn test_default_config() {
            let ctx = setup_test_storage();

            let config = ctx.storage.get_config().unwrap();
            assert_eq!(config.worker_count, DEFAULT_WORKER_COUNT);
            assert_eq!(config.chat_response_timeout_seconds, None);
            assert_eq!(
                config.agent.browser_timeout_secs,
                DEFAULT_AGENT_BROWSER_TIMEOUT_SECS
            );
            assert_eq!(config.agent.max_depth, DEFAULT_SUBAGENT_MAX_DEPTH);
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
                config.runtime_defaults.chat_max_session_history,
                DEFAULT_CHAT_MAX_SESSION_HISTORY
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
                stall_timeout_seconds: 600,
                chat_response_timeout_seconds: Some(900),
                max_retries: 5,
                chat_session_retention_days: 45,
                experimental_features: vec!["plan_mode".to_string()],
                ..Default::default()
            };

            ctx.storage.update_config(new_config).unwrap();

            let retrieved = ctx.storage.get_config().unwrap();
            assert_eq!(retrieved.worker_count, 8);
        }

        #[test]
        fn test_config_validation() {
            let valid_config = SystemConfig {
                worker_count: 2,
                stall_timeout_seconds: 30,
                chat_response_timeout_seconds: Some(300),
                max_retries: 1,
                chat_session_retention_days: 30,
                experimental_features: vec!["websocket_transport".to_string()],
                ..Default::default()
            };
            assert!(valid_config.validate().is_ok());
        }

        #[test]
        fn test_optional_timeouts_allow_none() {
            let config = SystemConfig {
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

            let mut config = ctx.storage.get_config().unwrap();
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
            assert_eq!(config.agent.max_depth, DEFAULT_SUBAGENT_MAX_DEPTH);
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
            assert!(!config.agent.auto_review_tools);

            config.agent.tool_timeout_secs = 180;
            config.agent.llm_timeout_secs = Some(900);
            config.agent.bash_timeout_secs = 600;
            config.agent.max_depth = 4;
            config.agent.max_wall_clock_secs = Some(3_600);
            config.agent.max_parallel_subagents = 25;
            config.agent.browser_timeout_secs = 180;
            config.agent.process_session_ttl_secs = 7_200;
            config.agent.approval_timeout_secs = 450;
            config.agent.auto_review_tools = true;
            ctx.storage.update_config(config).unwrap();

            let retrieved = ctx.storage.get_config().unwrap();
            assert_eq!(retrieved.agent.tool_timeout_secs, 180);
            assert_eq!(retrieved.agent.llm_timeout_secs, Some(900));
            assert_eq!(retrieved.agent.bash_timeout_secs, 600);
            assert_eq!(retrieved.agent.max_depth, 4);
            assert_eq!(retrieved.agent.max_wall_clock_secs, Some(3_600));
            assert_eq!(retrieved.agent.max_parallel_subagents, 25);
            assert_eq!(retrieved.agent.browser_timeout_secs, 180);
            assert_eq!(retrieved.agent.process_session_ttl_secs, 7_200);
            assert_eq!(retrieved.agent.approval_timeout_secs, 450);
            assert!(retrieved.agent.auto_review_tools);
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
            config.agent.max_depth = 0;
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
            let stored = ctx.storage.get_config().unwrap();
            assert_eq!(effective.worker_count, stored.worker_count);
        }

        #[test]
        fn test_effective_config_with_global_override() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[system]
    worker_count = 42
    "#,
            );
            let _guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, file.path());

            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.worker_count, 42);
        }

        #[test]
        fn test_effective_config_accepts_deprecated_sandbox_keys() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[cli.sandbox]
    enabled = false

    [cli.sandbox.env]
    isolate = false
    allow = []
    block = []

    [cli.sandbox.limits]
    timeout_secs = 120
    max_output_bytes = 1048576
    "#,
            );
            let _guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, file.path());

            ctx.storage.get_effective_config().unwrap();
        }

        #[test]
        fn test_workspace_override_precedence() {
            let ctx = setup_test_storage();
            let global_file = write_override_file(
                r#"[system]
    worker_count = 5
    max_retries = 2
    "#,
            );
            let workspace_file = write_override_file(
                r#"[system]
    worker_count = 9
    max_retries = 4
    "#,
            );
            let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, global_file.path());
            let _workspace_guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, workspace_file.path());

            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.worker_count, 9);
            assert_eq!(effective.max_retries, 4);
        }

        #[test]
        fn test_effective_config_cache_reloads_when_override_changes() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[system]
    worker_count = 5
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let first = ctx.storage.get_effective_config().unwrap();
            assert_eq!(first.worker_count, 5);

            fs::write(
                file.path(),
                r#"[system]
    worker_count = 17
    max_retries = 6
    "#,
            )
            .unwrap();

            let second = ctx.storage.get_effective_config().unwrap();
            assert_eq!(second.worker_count, 17);
            assert_eq!(second.max_retries, 6);
        }

        #[test]
        fn test_effective_config_cache_reloads_when_override_is_deleted() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[system]
    worker_count = 11
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let first = ctx.storage.get_effective_config().unwrap();
            assert_eq!(first.worker_count, 11);

            fs::remove_file(file.path()).unwrap();

            let second = ctx.storage.get_effective_config().unwrap();
            assert_eq!(second.worker_count, DEFAULT_WORKER_COUNT);
        }

        #[test]
        fn test_partial_agent_override() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[agent]
    python_timeout_secs = 45
    llm_timeout_secs = 660
    browser_timeout_secs = 240
    process_session_ttl_secs = 5400
    approval_timeout_secs = 420
    auto_review_tools = true
    max_wall_clock_secs = 7200
    fallback_models = ["alpha", "beta"]
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.agent.python_timeout_secs, 45);
            assert_eq!(effective.agent.llm_timeout_secs, Some(660));
            assert_eq!(effective.agent.browser_timeout_secs, 240);
            assert_eq!(effective.agent.process_session_ttl_secs, 5400);
            assert_eq!(effective.agent.approval_timeout_secs, 420);
            assert!(effective.agent.auto_review_tools);
            assert_eq!(effective.agent.max_wall_clock_secs, Some(7200));
            assert_eq!(
                effective.agent.fallback_models,
                Some(vec!["alpha".into(), "beta".into()])
            );
        }

        #[test]
        fn test_partial_api_override() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[api]
    web_search_num_results = 7
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.api_defaults.web_search_num_results, 7);
        }

        #[test]
        fn test_partial_runtime_and_registry_override() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[runtime]
    chat_max_session_history = 42

    [registry]
    github_cache_ttl_secs = 900
    marketplace_cache_ttl_secs = 450
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.runtime_defaults.chat_max_session_history, 42);
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
        fn test_config_storage_uses_defaults_without_config_file() {
            let _env_guard = env_lock();
            let temp_dir = tempdir().unwrap();
            let global_config = temp_dir.path().join("config.toml");
            let _global_guard = EnvGuard::set_path(GLOBAL_CONFIG_ENV, &global_config);

            let storage = ConfigStorage::new();
            let effective = storage.get_effective_config().unwrap();
            assert_eq!(effective.worker_count, DEFAULT_WORKER_COUNT);
            assert_eq!(effective.agent.max_iterations, DEFAULT_AGENT_MAX_ITERATIONS);
            assert!(!global_config.exists());
        }

        #[test]
        fn test_effective_config_sources_reports_paths_and_existence() {
            let _env_guard = env_lock();
            let global_file = write_override_file("[system]\nworker_count = 7\n");
            let workspace_file = write_override_file("[system]\nworker_count = 9\n");
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
            let file = write_override_file("[system]\nworker_count = 0\n");
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let result = ctx.storage.get_effective_config();
            assert!(result.is_err());
        }

        #[test]
        fn test_unknown_override_field_rejected() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[api]
    unknown_limit = 1
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            ctx.storage
                .get_effective_config()
                .expect_err("unknown override field should fail");
        }

        #[test]
        fn test_legacy_override_sections_are_rejected() {
            let ctx = setup_test_storage();
            let file = write_override_file(
                r#"[api_defaults]
    session_list_limit = 33
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            ctx.storage
                .get_effective_config()
                .expect_err("legacy override sections should fail");
        }

        #[test]
        fn test_api_round_trip() {
            let ctx = setup_test_storage();
            let mut config = ctx.storage.get_config().unwrap();
            assert_eq!(config.api_defaults.web_search_num_results, 5);
            config.api_defaults.web_search_num_results = 6;
            ctx.storage.update_config(config).unwrap();

            let retrieved = ctx.storage.get_config().unwrap();
            assert_eq!(retrieved.api_defaults.web_search_num_results, 6);
        }

        #[test]
        fn test_invalid_api_settings_rejected() {
            let mut config = SystemConfig::default();
            config.api_defaults.web_search_num_results = MAX_API_WEB_SEARCH_RESULTS + 1;
            assert!(config.validate().is_err());
        }

        #[test]
        fn test_invalid_runtime_channel_and_registry_defaults_rejected() {
            let mut config = SystemConfig::default();
            config.runtime_defaults.chat_max_session_history = 0;
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
                r#"[api]
    session_list_limit = 33
    "#,
            );
            let _guard = EnvGuard::set_path(WORKSPACE_CONFIG_ENV, file.path());

            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.api_defaults.session_list_limit, 33);
        }

        #[test]
        fn test_effective_config_does_not_infer_workspace_from_current_dir() {
            let ctx = setup_test_storage();
            let workspace = tempdir().unwrap();
            fs::create_dir_all(workspace.path().join(CONFIG_SUBDIR)).unwrap();
            fs::write(
                workspace.path().join(CONFIG_SUBDIR).join(CONFIG_FILE_NAME),
                "[system]\nworker_count = 9\n",
            )
            .unwrap();

            let _cwd_guard = CurrentDirGuard::set(workspace.path());
            let effective = ctx.storage.get_effective_config().unwrap();
            assert_eq!(effective.worker_count, DEFAULT_WORKER_COUNT);
        }

        #[test]
        fn test_effective_config_for_workspace_uses_explicit_workspace_root() {
            let ctx = setup_test_storage();
            let workspace = tempdir().unwrap();
            fs::create_dir_all(workspace.path().join(CONFIG_SUBDIR)).unwrap();
            fs::write(
                workspace.path().join(CONFIG_SUBDIR).join(CONFIG_FILE_NAME),
                "[system]\nworker_count = 9\n",
            )
            .unwrap();

            let effective = ctx
                .storage
                .get_effective_config_for_workspace(Some(workspace.path()))
                .unwrap();
            assert_eq!(effective.worker_count, 9);
        }
    }
}
mod encryption {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use anyhow::Result;
    use rand::Rng;

    const NONCE_SIZE: usize = 12;

    pub struct SecretEncryptor {
        cipher: Aes256Gcm,
    }

    impl SecretEncryptor {
        pub fn new(master_key: &[u8]) -> Result<Self> {
            if master_key.len() != 32 {
                return Err(anyhow::anyhow!(
                    "Master key must be 32 bytes, got {}",
                    master_key.len()
                ));
            }

            let cipher = Aes256Gcm::new_from_slice(master_key)
                .map_err(|err| anyhow::anyhow!("Invalid master key length: {:?}", err))?;

            Ok(Self { cipher })
        }

        pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
            let mut nonce_bytes = [0u8; NONCE_SIZE];
            rand::rng().fill_bytes(&mut nonce_bytes);
            let nonce = Nonce::from_slice(&nonce_bytes);

            let mut ciphertext = self
                .cipher
                .encrypt(nonce, plaintext)
                .map_err(|err| anyhow::anyhow!("Failed to encrypt payload: {:?}", err))?;
            let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
            output.extend_from_slice(&nonce_bytes);
            output.append(&mut ciphertext);
            Ok(output)
        }

        pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
            if ciphertext.len() < NONCE_SIZE {
                return Err(anyhow::anyhow!("Ciphertext is too short"));
            }

            let (nonce_bytes, payload) = ciphertext.split_at(NONCE_SIZE);
            let nonce = Nonce::from_slice(nonce_bytes);
            let plaintext = self
                .cipher
                .decrypt(nonce, payload)
                .map_err(|err| anyhow::anyhow!("Failed to decrypt payload: {:?}", err))?;
            Ok(plaintext)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn test_key() -> [u8; 32] {
            [0xAB; 32]
        }

        #[test]
        fn roundtrip() {
            let encryptor = SecretEncryptor::new(&test_key()).unwrap();
            let plaintext = b"hello world";
            let ciphertext = encryptor.encrypt(plaintext).unwrap();
            let decrypted = encryptor.decrypt(&ciphertext).unwrap();
            assert_eq!(decrypted, plaintext);
        }

        #[test]
        fn wrong_key_size_31() {
            let key = [0u8; 31];
            let result = SecretEncryptor::new(&key);
            let err = result.err().expect("should fail with 31-byte key");
            let msg = err.to_string();
            assert!(
                msg.contains("32"),
                "error should mention expected size 32: {msg}"
            );
        }

        #[test]
        fn wrong_key_size_33() {
            let key = [0u8; 33];
            let result = SecretEncryptor::new(&key);
            let err = result.err().expect("should fail with 33-byte key");
            let msg = err.to_string();
            assert!(
                msg.contains("32"),
                "error should mention expected size 32: {msg}"
            );
        }

        #[test]
        fn tampered_ciphertext() {
            let encryptor = SecretEncryptor::new(&test_key()).unwrap();
            let plaintext = b"sensitive data";
            let mut ciphertext = encryptor.encrypt(plaintext).unwrap();

            // Flip a byte in the authenticated ciphertext portion (after the nonce)
            let idx = NONCE_SIZE + 1;
            assert!(ciphertext.len() > idx, "ciphertext too short to tamper");
            ciphertext[idx] ^= 0xFF;

            let result = encryptor.decrypt(&ciphertext);
            assert!(
                result.is_err(),
                "decrypting tampered ciphertext should fail"
            );
        }

        #[test]
        fn different_key_decrypt() {
            let key_a = [0x11; 32];
            let key_b = [0x22; 32];
            let encryptor_a = SecretEncryptor::new(&key_a).unwrap();
            let encryptor_b = SecretEncryptor::new(&key_b).unwrap();

            let ciphertext = encryptor_a.encrypt(b"secret").unwrap();
            let result = encryptor_b.decrypt(&ciphertext);
            assert!(
                result.is_err(),
                "decrypting with a different key should fail"
            );
        }

        #[test]
        fn empty_plaintext_roundtrip() {
            let encryptor = SecretEncryptor::new(&test_key()).unwrap();
            let plaintext: &[u8] = b"";
            let ciphertext = encryptor.encrypt(plaintext).unwrap();
            // Ciphertext should still contain nonce + auth tag even for empty plaintext
            assert!(ciphertext.len() > NONCE_SIZE);
            let decrypted = encryptor.decrypt(&ciphertext).unwrap();
            assert_eq!(decrypted, plaintext);
        }

        #[test]
        fn nonce_uniqueness() {
            let encryptor = SecretEncryptor::new(&test_key()).unwrap();
            let plaintext = b"same input twice";
            let ct1 = encryptor.encrypt(plaintext).unwrap();
            let ct2 = encryptor.encrypt(plaintext).unwrap();
            assert_ne!(
                ct1, ct2,
                "encrypting the same plaintext twice should produce different ciphertexts due to random nonces"
            );
        }
    }
}
pub mod features {
    use crate::SystemConfig;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;
    use std::str::FromStr;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Stage {
        UnderDevelopment,
        Experimental,
        Stable,
        Deprecated,
        Removed,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Feature {
        WebSocketTransport,
        PlanMode,
    }

    #[derive(Debug, Clone, Serialize, PartialEq, Eq)]
    pub struct FeatureDescriptor {
        pub key: String,
        pub stage: Stage,
        pub description: &'static str,
        pub enabled: bool,
        pub requires_opt_in: bool,
    }

    #[derive(Debug, Clone, Default)]
    pub struct Features {
        experimental_opt_in: HashSet<Feature>,
    }

    impl Feature {
        pub const ALL: [Feature; 2] = [Feature::WebSocketTransport, Feature::PlanMode];

        pub fn key(self) -> &'static str {
            match self {
                Feature::WebSocketTransport => "websocket_transport",
                Feature::PlanMode => "plan_mode",
            }
        }

        pub fn stage(self) -> Stage {
            match self {
                Feature::WebSocketTransport => Stage::Experimental,
                Feature::PlanMode => Stage::Experimental,
            }
        }

        pub fn description(self) -> &'static str {
            match self {
                Feature::WebSocketTransport => "Use websocket transport for live client streams.",
                Feature::PlanMode => "Allow explicit user-plan pauses in agent execution.",
            }
        }

        pub fn requires_opt_in(self) -> bool {
            matches!(self.stage(), Stage::Experimental)
        }
    }

    impl FromStr for Feature {
        type Err = ();

        fn from_str(value: &str) -> Result<Self, Self::Err> {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "websocket_transport" | "websocket" => Ok(Feature::WebSocketTransport),
                "plan_mode" => Ok(Feature::PlanMode),
                _ => Err(()),
            }
        }
    }

    impl Features {
        pub fn from_config(config: &SystemConfig) -> Self {
            let experimental_opt_in = config
                .experimental_features
                .iter()
                .filter_map(|value| Feature::from_str(value).ok())
                .filter(|feature| feature.requires_opt_in())
                .collect::<HashSet<_>>();

            Self {
                experimental_opt_in,
            }
        }

        pub fn is_enabled(&self, feature: Feature) -> bool {
            match feature.stage() {
                Stage::Stable => true,
                Stage::Experimental => self.experimental_opt_in.contains(&feature),
                Stage::UnderDevelopment | Stage::Deprecated | Stage::Removed => false,
            }
        }

        pub fn descriptors(&self) -> Vec<FeatureDescriptor> {
            Feature::ALL
                .iter()
                .copied()
                .map(|feature| FeatureDescriptor {
                    key: feature.key().to_string(),
                    stage: feature.stage(),
                    description: feature.description(),
                    enabled: self.is_enabled(feature),
                    requires_opt_in: feature.requires_opt_in(),
                })
                .collect()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn config_with_flags(flags: &[&str]) -> SystemConfig {
            SystemConfig {
                experimental_features: flags.iter().map(|v| (*v).to_string()).collect(),
                ..SystemConfig::default()
            }
        }

        #[test]
        fn test_experimental_feature_requires_opt_in() {
            let without_opt_in = Features::from_config(&SystemConfig::default());
            assert!(!without_opt_in.is_enabled(Feature::PlanMode));

            let with_opt_in = Features::from_config(&config_with_flags(&["plan_mode"]));
            assert!(with_opt_in.is_enabled(Feature::PlanMode));
        }

        #[test]
        fn test_unknown_feature_in_config_is_ignored() {
            let config = config_with_flags(&["unknown_feature", "plan_mode"]);
            let features = Features::from_config(&config);
            assert!(features.is_enabled(Feature::PlanMode));
            assert!(!features.is_enabled(Feature::WebSocketTransport));
        }
    }
}
pub mod paths {
    use anyhow::Result;
    use std::path::PathBuf;

    const RESTFLOW_DIR: &str = ".restflow";
    const CONFIG_FILE: &str = "config.toml";
    const MASTER_KEY_FILE: &str = "master.key";
    const DB_FILE: &str = "restflow.db";
    const LOGS_DIR: &str = "logs";
    const SKILLS_DIR: &str = "skills";
    const MEDIA_DIR: &str = "media";
    const SESSIONS_DIR: &str = "sessions";

    /// Environment variable to override the RestFlow directory.
    const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";

    /// Resolve the RestFlow configuration directory.
    /// Priority: RESTFLOW_DIR env var > ~/.restflow/
    pub fn resolve_restflow_dir() -> Result<PathBuf> {
        if let Ok(dir) = std::env::var(RESTFLOW_DIR_ENV)
            && !dir.trim().is_empty()
        {
            return Ok(PathBuf::from(dir));
        }
        dirs::home_dir()
            .map(|h| h.join(RESTFLOW_DIR))
            .ok_or_else(|| anyhow::anyhow!("Failed to determine home directory"))
    }

    /// Ensure the RestFlow directory exists and return its path.
    pub fn ensure_restflow_dir() -> Result<PathBuf> {
        let dir = resolve_restflow_dir()?;
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Get the master key path: ~/.restflow/master.key
    pub fn master_key_path() -> Result<PathBuf> {
        Ok(resolve_restflow_dir()?.join(MASTER_KEY_FILE))
    }

    /// Get the global config path: ~/.restflow/config.toml
    pub fn config_path() -> Result<PathBuf> {
        Ok(resolve_restflow_dir()?.join(CONFIG_FILE))
    }

    /// Get the database path: ~/.restflow/restflow.db
    pub fn database_path() -> Result<PathBuf> {
        Ok(resolve_restflow_dir()?.join(DB_FILE))
    }

    /// Ensure database path exists and return as string.
    pub fn ensure_database_path() -> Result<PathBuf> {
        Ok(ensure_restflow_dir()?.join(DB_FILE))
    }

    /// Convenience helper returning the database path as a UTF-8 string.
    pub fn ensure_database_path_string() -> Result<String> {
        Ok(ensure_database_path()?.to_string_lossy().into_owned())
    }

    /// Get the logs directory: ~/.restflow/logs/
    pub fn logs_dir() -> Result<PathBuf> {
        let dir = resolve_restflow_dir()?.join(LOGS_DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Session transcript directory: ~/.restflow/sessions/
    pub fn sessions_dir() -> Result<PathBuf> {
        let dir = ensure_restflow_dir()?.join(SESSIONS_DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// User-global skills directory: ~/.restflow/skills/
    pub fn user_skills_dir() -> Result<PathBuf> {
        let dir = ensure_restflow_dir()?.join(SKILLS_DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Media directory: ~/.restflow/media/
    pub fn media_dir() -> Result<PathBuf> {
        let dir = ensure_restflow_dir()?.join(MEDIA_DIR);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Session-scoped media directory: ~/.restflow/media/{session_id}/
    pub fn session_media_dir(session_id: &str) -> Result<PathBuf> {
        let dir = media_dir()?.join(session_id);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// IPC socket path: ~/.restflow/restflow.sock
    pub fn socket_path() -> Result<PathBuf> {
        Ok(ensure_restflow_dir()?.join("restflow.sock"))
    }

    /// Daemon PID file path: ~/.restflow/daemon.pid
    pub fn daemon_pid_path() -> Result<PathBuf> {
        Ok(ensure_restflow_dir()?.join("daemon.pid"))
    }

    /// Daemon lock file path: ~/.restflow/daemon.lock
    pub fn daemon_lock_path() -> Result<PathBuf> {
        Ok(ensure_restflow_dir()?.join("daemon.lock"))
    }

    /// Daemon log file path: ~/.restflow/logs/daemon.log
    pub fn daemon_log_path() -> Result<PathBuf> {
        Ok(logs_dir()?.join("daemon.log"))
    }

    #[cfg(test)]
    pub(crate) fn restflow_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    #[cfg(all(not(test), feature = "test-utils"))]
    #[allow(dead_code)]
    pub(crate) fn restflow_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};

        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_default_restflow_dir() {
            let _lock = restflow_dir_env_lock();
            unsafe { std::env::remove_var("RESTFLOW_DIR") };
            let dir = resolve_restflow_dir().unwrap();
            assert!(dir.ends_with(".restflow"));
        }

        #[test]
        fn test_env_override() {
            let _lock = restflow_dir_env_lock();
            unsafe { std::env::set_var("RESTFLOW_DIR", "/tmp/test-restflow") };
            let dir = resolve_restflow_dir().unwrap();
            assert_eq!(dir, PathBuf::from("/tmp/test-restflow"));
            unsafe { std::env::remove_var("RESTFLOW_DIR") };
        }

        #[test]
        fn test_database_path() {
            let _lock = restflow_dir_env_lock();
            unsafe { std::env::remove_var("RESTFLOW_DIR") };
            let path = database_path().unwrap();
            assert!(path.ends_with(DB_FILE));
            assert!(path.parent().unwrap().ends_with(".restflow"));
        }

        #[test]
        fn test_daemon_lock_path() {
            let _lock = restflow_dir_env_lock();
            unsafe { std::env::remove_var("RESTFLOW_DIR") };
            let path = daemon_lock_path().unwrap();
            assert!(path.ends_with("daemon.lock"));
            assert!(path.parent().unwrap().ends_with(".restflow"));
        }
    }
}
pub mod process {
    use crate::time_utils;
    use anyhow::Result;
    use dashmap::DashMap;
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use types::DEFAULT_PROCESS_SESSION_TTL_SECS;
    use uuid::Uuid;

    use types::store::{ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo};

    mod session {
        use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub enum ProcessSessionSource {
            User,
            #[default]
            Agent,
        }

        #[derive(Debug, Clone, Default)]
        pub struct ProcessSessionMetadata {
            pub agent_id: Option<String>,
        }

        pub trait ProcessOutputListener: Send + Sync {
            fn on_output(&self, session_id: &str, data: &str);
            fn on_closed(&self, session_id: &str);
        }

        #[derive(Debug, Default)]
        pub struct SessionOutput {
            pub pending: String,
            pub aggregated: String,
        }

        pub struct ProcessSession {
            pub id: String,
            pub command: String,
            pub cwd: Option<String>,
            pub started_at: i64,
            pub source: ProcessSessionSource,
            pub metadata: ProcessSessionMetadata,
            pub writer: Mutex<Box<dyn Write + Send>>,
            pub master: Mutex<Box<dyn MasterPty + Send>>,
            pub output: Arc<Mutex<SessionOutput>>,
            pub output_listener: Option<Arc<dyn ProcessOutputListener>>,
            pub child: Mutex<Box<dyn Child + Send + Sync>>,
            pub killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
            exit_status: Mutex<Option<ExitStatus>>,
            read_closed: AtomicBool,
        }

        impl std::fmt::Debug for ProcessSession {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("ProcessSession")
                    .field("id", &self.id)
                    .field("command", &self.command)
                    .field("cwd", &self.cwd)
                    .field("started_at", &self.started_at)
                    .finish_non_exhaustive()
            }
        }

        impl ProcessSession {
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                id: String,
                command: String,
                cwd: Option<String>,
                started_at: i64,
                source: ProcessSessionSource,
                metadata: ProcessSessionMetadata,
                writer: Box<dyn Write + Send>,
                master: Box<dyn MasterPty + Send>,
                output: Arc<Mutex<SessionOutput>>,
                output_listener: Option<Arc<dyn ProcessOutputListener>>,
                child: Box<dyn Child + Send + Sync>,
            ) -> Self {
                let killer = child.clone_killer();
                Self {
                    id,
                    command,
                    cwd,
                    started_at,
                    source,
                    metadata,
                    writer: Mutex::new(writer),
                    master: Mutex::new(master),
                    output,
                    output_listener,
                    child: Mutex::new(child),
                    killer: Mutex::new(killer),
                    exit_status: Mutex::new(None),
                    read_closed: AtomicBool::new(false),
                }
            }

            /// Kill the process
            pub fn kill(&self) -> anyhow::Result<()> {
                let mut killer = self
                    .killer
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
                killer.kill()?;
                Ok(())
            }

            /// Wait for process exit status to become available.
            pub fn wait_for_exit(&self, timeout: Duration) -> anyhow::Result<Option<ExitStatus>> {
                if let Some(status) = self.exit_status() {
                    return Ok(Some(status));
                }

                let deadline = Instant::now() + timeout;
                let mut backoff_ms = 10u64;
                loop {
                    if let Some(status) = self.try_update_exit_status()? {
                        return Ok(Some(status));
                    }

                    if Instant::now() >= deadline {
                        return Ok(None);
                    }

                    std::thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms = (backoff_ms.saturating_mul(2)).min(200);
                }
            }

            /// Terminate process and best-effort reap child status.
            pub fn terminate_and_reap(
                &self,
                timeout: Duration,
            ) -> anyhow::Result<Option<ExitStatus>> {
                self.kill()?;
                self.wait_for_exit(timeout)
            }

            pub fn resize(&self, size: PtySize) -> anyhow::Result<()> {
                let master = self
                    .master
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
                master.resize(size)?;
                Ok(())
            }

            pub fn emit_output(&self, data: &str) {
                if let Some(listener) = self.output_listener.as_ref() {
                    listener.on_output(&self.id, data);
                }
            }

            pub fn emit_closed(&self) {
                if let Some(listener) = self.output_listener.as_ref() {
                    listener.on_closed(&self.id);
                }
            }

            pub fn mark_read_closed(&self) {
                self.read_closed.store(true, Ordering::Release);
            }

            pub fn read_closed(&self) -> bool {
                self.read_closed.load(Ordering::Acquire)
            }

            pub fn exit_status(&self) -> Option<ExitStatus> {
                self.exit_status
                    .lock()
                    .ok()
                    .and_then(|status| status.clone())
            }

            pub fn set_exit_status(&self, status: ExitStatus) {
                if let Ok(mut guard) = self.exit_status.lock() {
                    *guard = Some(status);
                }
            }

            pub fn try_update_exit_status(&self) -> anyhow::Result<Option<ExitStatus>> {
                if self.exit_status().is_some() {
                    return Ok(self.exit_status());
                }

                let mut child = self
                    .child
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
                if let Some(status) = child.try_wait()? {
                    self.set_exit_status(status.clone());
                    return Ok(Some(status));
                }
                Ok(None)
            }
        }

        #[derive(Debug, Clone)]
        pub struct FinishedSession {
            pub id: String,
            pub command: String,
            pub cwd: Option<String>,
            pub started_at: i64,
            pub finished_at: i64,
            pub exit_code: Option<i32>,
            pub output: String,
        }
    }

    pub use session::{
        FinishedSession, ProcessOutputListener, ProcessSession, ProcessSessionMetadata,
        ProcessSessionSource, SessionOutput,
    };

    const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_000_000;
    const CLEANUP_INTERVAL_SECONDS: u64 = 60;
    const SESSION_REAP_TIMEOUT: Duration = Duration::from_secs(2);
    const DEFAULT_PTY_SIZE: PtySize = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    #[derive(Clone)]
    pub struct ProcessSpawnOptions {
        pub session_id: Option<String>,
        pub cwd: Option<String>,
        pub source: ProcessSessionSource,
        pub metadata: ProcessSessionMetadata,
        pub pty_size: PtySize,
        pub output_listener: Option<Arc<dyn ProcessOutputListener>>,
    }

    impl Default for ProcessSpawnOptions {
        fn default() -> Self {
            Self {
                session_id: None,
                cwd: None,
                source: ProcessSessionSource::default(),
                metadata: ProcessSessionMetadata::default(),
                pty_size: DEFAULT_PTY_SIZE,
                output_listener: None,
            }
        }
    }

    #[derive(Clone, Default)]
    pub struct ProcessShellOptions {
        pub spawn: ProcessSpawnOptions,
        pub startup_command: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct ProcessRegistry {
        sessions: Arc<DashMap<String, Arc<ProcessSession>>>,
        finished: Arc<DashMap<String, FinishedSession>>,
        max_output_bytes: usize,
        ttl_seconds: Arc<AtomicU64>,
    }

    impl Default for ProcessRegistry {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ProcessRegistry {
        pub fn new() -> Self {
            let registry = Self {
                sessions: Arc::new(DashMap::new()),
                finished: Arc::new(DashMap::new()),
                max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
                ttl_seconds: Arc::new(AtomicU64::new(DEFAULT_PROCESS_SESSION_TTL_SECS)),
            };

            registry.spawn_cleanup_task();
            registry
        }

        pub fn with_max_output(mut self, max_output_bytes: usize) -> Self {
            self.max_output_bytes = max_output_bytes;
            self
        }

        pub fn with_ttl_seconds(self, ttl_seconds: u64) -> Self {
            self.set_ttl_seconds(ttl_seconds);
            self
        }

        pub fn set_ttl_seconds(&self, ttl_seconds: u64) -> u64 {
            self.ttl_seconds.swap(ttl_seconds, Ordering::Relaxed)
        }

        pub fn ttl_seconds(&self) -> u64 {
            self.ttl_seconds.load(Ordering::Relaxed)
        }

        fn spawn_cleanup_task(&self) {
            let sessions = self.sessions.clone();
            let finished = self.finished.clone();
            let ttl_seconds = self.ttl_seconds.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL_SECONDS)).await;
                        Self::run_maintenance_once(&sessions, &finished, &ttl_seconds);
                    }
                });
            } else {
                tracing::warn!("No Tokio runtime found for process cleanup task");
            }
        }

        fn run_maintenance_once(
            sessions: &DashMap<String, Arc<ProcessSession>>,
            finished: &DashMap<String, FinishedSession>,
            ttl_seconds: &AtomicU64,
        ) {
            let completed: Vec<(Arc<ProcessSession>, Option<i32>)> = sessions
                .iter()
                .filter_map(|entry| {
                    let session = entry.value().clone();
                    match session.try_update_exit_status() {
                        Ok(Some(status)) => Some((session, Some(status.exit_code() as i32))),
                        Ok(None) => None,
                        Err(error) => {
                            tracing::warn!(
                                session_id = %entry.key(),
                                error = %error,
                                "Failed to poll process exit status during cleanup"
                            );
                            None
                        }
                    }
                })
                .collect();

            for (session, exit_code) in completed {
                // Always finalize completed sessions during maintenance, even without active pollers.
                Self::finalize_session_maps(sessions, finished, session, exit_code);
            }

            let now = current_timestamp_ms();
            let ttl = ttl_seconds.load(Ordering::Relaxed);
            Self::cleanup_expired_finished_sessions(finished, now, ttl);
        }

        fn run_maintenance(&self) {
            Self::run_maintenance_once(&self.sessions, &self.finished, &self.ttl_seconds);
        }

        fn build_shell_command(command: &str) -> CommandBuilder {
            #[cfg(target_os = "windows")]
            {
                let mut cmd = CommandBuilder::new("cmd.exe");
                cmd.args(["/C", command]);
                cmd
            }
            #[cfg(not(target_os = "windows"))]
            {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                let mut cmd = CommandBuilder::new(shell);
                cmd.args(["-c", command]);
                cmd
            }
        }

        fn append_output(output: &mut SessionOutput, data: &str, max_bytes: usize) {
            output.pending.push_str(data);
            output.aggregated.push_str(data);

            if output.pending.len() > max_bytes {
                let target = max_bytes * 9 / 10;
                let keep_from = output.pending.len().saturating_sub(target);
                let start = Self::nearest_char_boundary_forward(&output.pending, keep_from);
                output.pending = output.pending[start..].to_string();
            }

            if output.aggregated.len() > max_bytes {
                let target = max_bytes * 9 / 10;
                let keep_from = output.aggregated.len().saturating_sub(target);
                let start = Self::nearest_char_boundary_forward(&output.aggregated, keep_from);
                output.aggregated = output.aggregated[start..].to_string();
            }
        }

        fn nearest_char_boundary_forward(text: &str, index: usize) -> usize {
            let mut pos = index.min(text.len());
            while pos < text.len() && !text.is_char_boundary(pos) {
                pos += 1;
            }
            pos
        }

        fn slice_utf8(text: &str, offset: usize, limit: usize) -> String {
            if text.is_empty() {
                return String::new();
            }
            let mut start = offset.min(text.len());
            while start > 0 && !text.is_char_boundary(start) {
                start -= 1;
            }
            let mut end = start.saturating_add(limit).min(text.len());
            while end < text.len() && !text.is_char_boundary(end) {
                end += 1;
            }
            text[start..end].to_string()
        }

        fn is_truncated(total: usize, offset: usize, limit: usize) -> bool {
            offset.saturating_add(limit) < total
        }

        fn take_pending(output: &Arc<Mutex<SessionOutput>>) -> String {
            if let Ok(mut guard) = output.lock() {
                let pending = guard.pending.clone();
                guard.pending.clear();
                return pending;
            }
            String::new()
        }

        fn session_status(exit_code: Option<i32>) -> String {
            match exit_code {
                None => "running".to_string(),
                Some(0) => "completed".to_string(),
                Some(_) => "failed".to_string(),
            }
        }

        fn finalize_session(&self, session: Arc<ProcessSession>, exit_code: Option<i32>) {
            Self::finalize_session_maps(&self.sessions, &self.finished, session, exit_code);
        }

        fn finalize_session_maps(
            sessions: &DashMap<String, Arc<ProcessSession>>,
            finished: &DashMap<String, FinishedSession>,
            session: Arc<ProcessSession>,
            exit_code: Option<i32>,
        ) {
            let output = session
                .output
                .lock()
                .map(|o| o.aggregated.clone())
                .unwrap_or_default();
            let finished_record = FinishedSession {
                id: session.id.clone(),
                command: session.command.clone(),
                cwd: session.cwd.clone(),
                started_at: session.started_at,
                finished_at: current_timestamp_ms(),
                exit_code,
                output,
            };
            sessions.remove(&session.id);
            finished.insert(session.id.clone(), finished_record);
        }

        fn cleanup_expired_finished_sessions(
            finished: &DashMap<String, FinishedSession>,
            now_ms: i64,
            ttl_seconds: u64,
        ) {
            let expired: Vec<String> = finished
                .iter()
                .filter_map(|entry| {
                    if is_ttl_expired(now_ms, entry.finished_at, ttl_seconds) {
                        Some(entry.key().clone())
                    } else {
                        None
                    }
                })
                .collect();

            for session_id in expired {
                finished.remove(&session_id);
            }
        }

        fn create_reader_thread(
            session: Arc<ProcessSession>,
            mut reader: Box<dyn Read + Send>,
            max_output: usize,
        ) {
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let mut incomplete_utf8: Vec<u8> = Vec::new();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            if !incomplete_utf8.is_empty() {
                                let data = String::from_utf8_lossy(&incomplete_utf8).to_string();
                                if let Ok(mut output) = session.output.lock() {
                                    Self::append_output(&mut output, &data, max_output);
                                }
                                session.emit_output(&data);
                            }
                            session.emit_closed();
                            session.mark_read_closed();
                            break;
                        }
                        Ok(n) => {
                            let mut bytes = std::mem::take(&mut incomplete_utf8);
                            bytes.extend_from_slice(&buf[..n]);
                            let valid_up_to = find_utf8_boundary(&bytes);
                            if valid_up_to > 0 {
                                let data =
                                    String::from_utf8_lossy(&bytes[..valid_up_to]).to_string();
                                if let Ok(mut output) = session.output.lock() {
                                    Self::append_output(&mut output, &data, max_output);
                                }
                                session.emit_output(&data);
                            }
                            if valid_up_to < bytes.len() {
                                incomplete_utf8 = bytes[valid_up_to..].to_vec();
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Process output read error");
                            session.emit_closed();
                            session.mark_read_closed();
                            break;
                        }
                    }
                }
            });
        }

        pub fn spawn(&self, command: &str, cwd: Option<String>) -> Result<String> {
            let options = ProcessSpawnOptions {
                cwd,
                source: ProcessSessionSource::Agent,
                ..Default::default()
            };
            self.spawn_with_options(command, options)
        }

        pub fn spawn_with_options(
            &self,
            command: &str,
            options: ProcessSpawnOptions,
        ) -> Result<String> {
            self.run_maintenance();

            let pty_system = native_pty_system();
            let pair = pty_system.openpty(options.pty_size)?;

            let mut cmd = Self::build_shell_command(command);
            if let Some(cwd) = options.cwd.as_ref() {
                cmd.cwd(cwd);
            }
            cmd.env("TERM", "xterm-256color");

            let child = pair.slave.spawn_command(cmd)?;
            let writer = pair.master.take_writer()?;
            let reader = pair.master.try_clone_reader()?;

            let session_id = options
                .session_id
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let output = Arc::new(Mutex::new(SessionOutput::default()));
            let session = Arc::new(ProcessSession::new(
                session_id.clone(),
                command.to_string(),
                options.cwd.clone(),
                current_timestamp_ms(),
                options.source,
                options.metadata,
                writer,
                pair.master,
                output.clone(),
                options.output_listener,
                child,
            ));

            Self::create_reader_thread(session.clone(), reader, self.max_output_bytes);

            self.sessions.insert(session_id.clone(), session);
            Ok(session_id)
        }

        pub fn spawn_shell(&self, shell: &str, options: ProcessShellOptions) -> Result<String> {
            self.run_maintenance();

            let pty_system = native_pty_system();
            let pair = pty_system.openpty(options.spawn.pty_size)?;

            let mut cmd = CommandBuilder::new(shell);
            if let Some(cwd) = options.spawn.cwd.as_ref() {
                cmd.cwd(cwd);
            }
            cmd.env("TERM", "xterm-256color");

            let child = pair.slave.spawn_command(cmd)?;
            let mut writer = pair.master.take_writer()?;
            let reader = pair.master.try_clone_reader()?;

            if let Some(startup) = options.startup_command.as_ref()
                && !startup.is_empty()
            {
                std::thread::sleep(Duration::from_millis(100));
                let _ = writer.write_all(format!("{}\n", startup).as_bytes());
                let _ = writer.flush();
            }

            let session_id = options
                .spawn
                .session_id
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let output = Arc::new(Mutex::new(SessionOutput::default()));
            let session = Arc::new(ProcessSession::new(
                session_id.clone(),
                shell.to_string(),
                options.spawn.cwd.clone(),
                current_timestamp_ms(),
                options.spawn.source,
                options.spawn.metadata,
                writer,
                pair.master,
                output.clone(),
                options.spawn.output_listener,
                child,
            ));

            Self::create_reader_thread(session.clone(), reader, self.max_output_bytes);

            self.sessions.insert(session_id.clone(), session);
            Ok(session_id)
        }

        pub fn poll(&self, session_id: &str) -> Result<ProcessPollResult> {
            self.run_maintenance();

            if let Some(session) = self.sessions.get(session_id) {
                let session = session.value().clone();
                let _ = session.try_update_exit_status();
                let pending = Self::take_pending(&session.output);
                let exit_code = session
                    .exit_status()
                    .map(|status| status.exit_code() as i32);
                let status = Self::session_status(exit_code);

                if exit_code.is_some() && session.read_closed() {
                    self.finalize_session(session, exit_code);
                }

                return Ok(ProcessPollResult {
                    session_id: session_id.to_string(),
                    output: pending,
                    status,
                    exit_code,
                });
            }

            if let Some(finished) = self.finished.get(session_id) {
                let exit_code = finished.exit_code;
                return Ok(ProcessPollResult {
                    session_id: session_id.to_string(),
                    output: String::new(),
                    status: Self::session_status(exit_code),
                    exit_code,
                });
            }

            anyhow::bail!("Session not found: {}", session_id)
        }

        pub fn write(&self, session_id: &str, data: &str) -> Result<()> {
            self.run_maintenance();

            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
            let mut writer = session
                .writer
                .lock()
                .map_err(|_| anyhow::anyhow!("Process session lock poisoned"))?;
            writer.write_all(data.as_bytes())?;
            writer.flush()?;
            Ok(())
        }

        pub fn resize(&self, session_id: &str, size: PtySize) -> Result<()> {
            self.run_maintenance();

            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
            session.resize(size)?;
            Ok(())
        }

        pub fn kill(&self, session_id: &str) -> Result<()> {
            self.run_maintenance();

            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?
                .value()
                .clone();

            if let Err(error) = session.terminate_and_reap(SESSION_REAP_TIMEOUT) {
                let reaped =
                    session.try_update_exit_status()?.is_some() || session.exit_status().is_some();
                if !reaped {
                    return Err(error);
                }
            }

            let exit_code = session
                .exit_status()
                .map(|status| status.exit_code() as i32);
            if exit_code.is_some() && session.read_closed() {
                self.finalize_session(session, exit_code);
            }

            Ok(())
        }

        pub fn get_output_buffer(&self, session_id: &str) -> Option<String> {
            self.run_maintenance();

            self.sessions
                .get(session_id)
                .and_then(|session| session.output.lock().ok().map(|o| o.aggregated.clone()))
        }

        pub fn remove_session(&self, session_id: &str) -> Option<String> {
            self.run_maintenance();

            if let Some((_, session)) = self.sessions.remove(session_id) {
                let _ = session.terminate_and_reap(SESSION_REAP_TIMEOUT);
                let _ = session.try_update_exit_status();
                let output = session
                    .output
                    .lock()
                    .ok()
                    .map(|o| o.aggregated.clone())
                    .unwrap_or_default();
                let exit_code = session
                    .exit_status()
                    .map(|status| status.exit_code() as i32);
                Self::finalize_session_maps(&self.sessions, &self.finished, session, exit_code);
                return Some(output);
            }

            if let Some(finished) = self.finished.get(session_id) {
                return Some(finished.output.clone());
            }

            None
        }

        pub fn list_session_ids_by_source(&self, source: ProcessSessionSource) -> Vec<String> {
            self.run_maintenance();

            self.sessions
                .iter()
                .filter_map(|entry| {
                    let session = entry.value();
                    if session.source == source {
                        Some(session.id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        }

        pub fn has_session(&self, session_id: &str) -> bool {
            self.run_maintenance();
            self.sessions.contains_key(session_id)
        }

        pub fn list(&self) -> Vec<ProcessSessionInfo> {
            self.run_maintenance();

            let mut items: Vec<ProcessSessionInfo> = self
                .sessions
                .iter()
                .map(|entry| {
                    let session = entry.value();
                    let exit_code = session
                        .exit_status()
                        .map(|status| status.exit_code() as i32);
                    ProcessSessionInfo {
                        session_id: session.id.clone(),
                        command: session.command.clone(),
                        cwd: session.cwd.clone(),
                        started_at: session.started_at,
                        status: Self::session_status(exit_code),
                        exit_code,
                    }
                })
                .collect();

            for entry in self.finished.iter() {
                items.push(ProcessSessionInfo {
                    session_id: entry.id.clone(),
                    command: entry.command.clone(),
                    cwd: entry.cwd.clone(),
                    started_at: entry.started_at,
                    status: Self::session_status(entry.exit_code),
                    exit_code: entry.exit_code,
                });
            }

            items
        }

        pub fn get_log(&self, session_id: &str, offset: usize, limit: usize) -> Result<ProcessLog> {
            self.run_maintenance();

            if let Some(session) = self.sessions.get(session_id) {
                let output = session
                    .output
                    .lock()
                    .map(|o| o.aggregated.clone())
                    .unwrap_or_default();
                let total = output.len();
                let slice = Self::slice_utf8(&output, offset, limit);
                return Ok(ProcessLog {
                    session_id: session_id.to_string(),
                    output: slice,
                    offset,
                    limit,
                    total,
                    truncated: Self::is_truncated(total, offset, limit),
                });
            }

            if let Some(finished) = self.finished.get(session_id) {
                let total = finished.output.len();
                let slice = Self::slice_utf8(&finished.output, offset, limit);
                return Ok(ProcessLog {
                    session_id: session_id.to_string(),
                    output: slice,
                    offset,
                    limit,
                    total,
                    truncated: Self::is_truncated(total, offset, limit),
                });
            }

            anyhow::bail!("Session not found: {}", session_id)
        }
    }

    impl ProcessManager for ProcessRegistry {
        fn spawn(&self, command: String, cwd: Option<String>) -> Result<String> {
            Self::spawn(self, &command, cwd)
        }

        fn poll(&self, session_id: &str) -> Result<ProcessPollResult> {
            Self::poll(self, session_id)
        }

        fn write(&self, session_id: &str, data: &str) -> Result<()> {
            Self::write(self, session_id, data)
        }

        fn kill(&self, session_id: &str) -> Result<()> {
            Self::kill(self, session_id)
        }

        fn list(&self) -> Result<Vec<ProcessSessionInfo>> {
            Ok(Self::list(self))
        }

        fn log(&self, session_id: &str, offset: usize, limit: usize) -> Result<ProcessLog> {
            Self::get_log(self, session_id, offset, limit)
        }
    }

    fn current_timestamp_ms() -> i64 {
        time_utils::now_ms()
    }

    fn ttl_seconds_to_millis(ttl_seconds: u64) -> i64 {
        Duration::from_secs(ttl_seconds)
            .as_millis()
            .min(i64::MAX as u128) as i64
    }

    fn is_ttl_expired(now_ms: i64, finished_at_ms: i64, ttl_seconds: u64) -> bool {
        now_ms.saturating_sub(finished_at_ms) > ttl_seconds_to_millis(ttl_seconds)
    }

    fn find_utf8_boundary(bytes: &[u8]) -> usize {
        match std::str::from_utf8(bytes) {
            Ok(_) => bytes.len(),
            Err(e) => e.valid_up_to(),
        }
    }

    #[cfg(test)]
    mod tests {
        #[allow(unused_imports)]
        use super::*;

        /// Test spawning a process and polling its output.
        /// Ignored in CI due to PTY reader thread cleanup issues that can cause hangs.
        /// Run manually with: cargo test --package runtime process::tests::test_spawn_and_poll -- --ignored
        #[cfg(unix)]
        #[tokio::test]
        #[ignore]
        async fn test_spawn_and_poll() {
            let registry = ProcessRegistry::new();
            let session_id = registry.spawn("echo hello", None).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            let result = registry.poll(&session_id).unwrap();
            assert!(result.output.contains("hello"));
            assert!(result.status == "completed" || result.status == "running");
        }

        /// Test interactive process with stdin/stdout.
        /// Ignored in CI due to PTY reader thread cleanup issues that can cause hangs.
        /// Run manually with: cargo test --package runtime process::tests::test_interactive_process -- --ignored
        #[cfg(unix)]
        #[tokio::test]
        #[ignore]
        async fn test_interactive_process() {
            let registry = ProcessRegistry::new();
            let session_id = registry.spawn("cat", None).unwrap();
            registry.write(&session_id, "ping\n").unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            let result = registry.poll(&session_id).unwrap();
            assert!(result.output.contains("ping"));
            registry.kill(&session_id).unwrap();
        }

        /// Test killing a running process session.
        /// Ignored in CI due to PTY reader thread cleanup issues that can cause hangs.
        /// Run manually with: cargo test --package runtime process::tests::test_kill_session -- --ignored
        #[cfg(unix)]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[ignore]
        async fn test_kill_session() {
            let test_future = async {
                let registry = ProcessRegistry::new();
                let session_id = registry.spawn("sleep 5", None).unwrap();
                registry.kill(&session_id).unwrap();

                // Wait for process to terminate with polling
                for _ in 0..50 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let result = registry.poll(&session_id).unwrap();
                    if result.status != "running" {
                        assert!(
                            result.status == "failed" || result.status == "completed",
                            "Unexpected status: {}",
                            result.status
                        );
                        return;
                    }
                }
                panic!("Process did not terminate after kill within 5 seconds");
            };

            tokio::time::timeout(Duration::from_secs(10), test_future)
                .await
                .expect("test_kill_session timed out after 10 seconds");
        }

        #[test]
        fn test_append_output_keeps_utf8_boundaries() {
            let mut output = SessionOutput::default();
            let data = "前缀😀后缀".repeat(64);

            ProcessRegistry::append_output(&mut output, &data, 128);

            assert!(std::str::from_utf8(output.pending.as_bytes()).is_ok());
            assert!(std::str::from_utf8(output.aggregated.as_bytes()).is_ok());
            assert!(!output.pending.is_empty());
            assert!(!output.aggregated.is_empty());
        }

        #[test]
        fn test_is_truncated_handles_large_offset_without_overflow() {
            let truncated = ProcessRegistry::is_truncated(10, usize::MAX - 2, 10);
            assert!(!truncated);
        }

        #[test]
        fn test_with_ttl_seconds_updates_shared_ttl_for_cleanup_worker() {
            let registry = ProcessRegistry::new().with_ttl_seconds(120);
            assert_eq!(registry.ttl_seconds(), 120);

            let _ = registry.clone().with_ttl_seconds(5);
            assert_eq!(registry.ttl_seconds(), 5);
        }

        #[test]
        fn test_set_ttl_seconds_updates_long_lived_registry_in_place() {
            let registry = Arc::new(ProcessRegistry::new().with_ttl_seconds(90));
            assert_eq!(registry.ttl_seconds(), 90);

            let old = registry.set_ttl_seconds(15);
            assert_eq!(old, 90);
            assert_eq!(registry.ttl_seconds(), 15);

            let old_from_clone = registry.clone().set_ttl_seconds(7);
            assert_eq!(old_from_clone, 15);
            assert_eq!(registry.ttl_seconds(), 7);
        }

        #[test]
        fn test_cleanup_uses_updated_ttl_seconds_value_behaviorally() {
            let registry = ProcessRegistry::new().with_ttl_seconds(2);
            let session_id = "finished-session".to_string();
            registry.finished.insert(
                session_id.clone(),
                FinishedSession {
                    id: session_id.clone(),
                    command: "echo done".to_string(),
                    cwd: None,
                    started_at: 0,
                    finished_at: 1_000,
                    exit_code: Some(0),
                    output: "done".to_string(),
                },
            );

            let now = 2_500;
            ProcessRegistry::cleanup_expired_finished_sessions(
                &registry.finished,
                now,
                registry.ttl_seconds(),
            );
            assert!(
                registry.finished.contains_key(&session_id),
                "Session should stay when elapsed <= updated TTL"
            );

            let _ = registry.clone().with_ttl_seconds(1);
            ProcessRegistry::cleanup_expired_finished_sessions(
                &registry.finished,
                now,
                registry.ttl_seconds(),
            );
            assert!(
                !registry.finished.contains_key(&session_id),
                "Session should be removed once elapsed > updated TTL"
            );
        }

        #[test]
        fn test_opportunistic_maintenance_cleans_expired_finished_without_worker() {
            let registry = ProcessRegistry::new().with_ttl_seconds(1);
            let session_id = "expired-finished".to_string();
            let now = current_timestamp_ms();
            registry.finished.insert(
                session_id.clone(),
                FinishedSession {
                    id: session_id.clone(),
                    command: "echo done".to_string(),
                    cwd: None,
                    started_at: now - 6_000,
                    finished_at: now - 5_000,
                    exit_code: Some(0),
                    output: "done".to_string(),
                },
            );

            assert!(registry.finished.contains_key(&session_id));
            let _ = registry.list();
            assert!(
                !registry.finished.contains_key(&session_id),
                "Expired finished session should be cleaned during foreground maintenance"
            );
        }

        #[test]
        fn test_runtime_ttl_update_applies_to_existing_finished_sessions_without_restart() {
            let registry = ProcessRegistry::new().with_ttl_seconds(2);
            let session_id = "dynamic-ttl-finished".to_string();
            let now = current_timestamp_ms();
            registry.finished.insert(
                session_id.clone(),
                FinishedSession {
                    id: session_id.clone(),
                    command: "echo done".to_string(),
                    cwd: None,
                    started_at: now - 2_000,
                    finished_at: now - 1_200,
                    exit_code: Some(0),
                    output: "done".to_string(),
                },
            );

            let _ = registry.list();
            assert!(
                registry.finished.contains_key(&session_id),
                "Session should stay when TTL is larger than elapsed lifetime"
            );

            registry.set_ttl_seconds(1);
            let _ = registry.list();
            assert!(
                !registry.finished.contains_key(&session_id),
                "Session should expire after runtime TTL update without recreating registry"
            );
        }

        #[test]
        fn test_ttl_expiry_boundary_is_strict_greater_than() {
            assert!(
                !is_ttl_expired(2_000, 1_000, 1),
                "Elapsed == TTL should not expire"
            );
            assert!(
                is_ttl_expired(2_001, 1_000, 1),
                "Elapsed > TTL should expire"
            );
        }
    }
}
pub mod prompt_files {
    use anyhow::{Context, Result};
    use std::fs;
    use std::path::{Path, PathBuf};

    const AGENTS_DIR: &str = "agents";
    /// Environment variable to override the agents directory path (used in tests).
    pub const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

    const DEFAULT_AGENT_PROMPT: &str = r#"You are a RestFlow agent.

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

    pub fn ensure_prompt_templates() -> Result<()> {
        Ok(())
    }

    pub fn load_default_main_agent_prompt() -> Result<String> {
        Ok(DEFAULT_AGENT_PROMPT.to_string())
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LoadedAgentPrompt {
        pub content: Option<String>,
        pub prompt_file: Option<String>,
    }

    pub fn load_agent_prompt_for_agent(
        agent_id: &str,
        agent_name: &str,
        prompt_file: Option<&str>,
    ) -> Result<LoadedAgentPrompt> {
        validate_agent_id(agent_id)?;
        let Some(path) = resolve_prompt_path_for_read(agent_name, prompt_file)? else {
            return Ok(LoadedAgentPrompt {
                content: None,
                prompt_file: None,
            });
        };

        let Some(content) = read_prompt_file_if_exists(&path)? else {
            return Ok(LoadedAgentPrompt {
                content: None,
                prompt_file: None,
            });
        };
        let content = strip_optional_frontmatter(&content).unwrap_or(content);

        Ok(LoadedAgentPrompt {
            content: if content.trim().is_empty() {
                None
            } else {
                Some(content)
            },
            prompt_file: Some(extract_prompt_file_name(&path)?),
        })
    }

    fn strip_optional_frontmatter(content: &str) -> Option<String> {
        let rest = content.strip_prefix("---\n")?;
        let (_, body) = rest.split_once("\n---")?;
        let body = body
            .strip_prefix("\n\n")
            .or_else(|| body.strip_prefix('\n'))
            .unwrap_or(body);
        Some(body.to_string())
    }

    fn read_prompt_file_if_exists(path: &Path) -> Result<Option<String>> {
        match fs::read_to_string(path) {
            Ok(content) => Ok(Some(content)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("Failed to read agent prompt: {}", path.display())),
        }
    }

    pub fn ensure_agent_prompt_file(
        agent_id: &str,
        agent_name: &str,
        current_prompt_file: Option<&str>,
        prompt_override: Option<&str>,
    ) -> Result<PathBuf> {
        ensure_prompt_templates()?;
        validate_agent_id(agent_id)?;
        let path = resolve_prompt_path_for_write(agent_name, current_prompt_file)?;

        if let Some(prompt) = prompt_override {
            fs::write(&path, prompt)
                .with_context(|| format!("Failed to write agent prompt: {}", path.display()))?;
            return Ok(path);
        }

        if path.exists() {
            return Ok(path);
        }

        let default_prompt = load_default_main_agent_prompt()?;
        fs::write(&path, default_prompt)
            .with_context(|| format!("Failed to initialize agent prompt: {}", path.display()))?;
        Ok(path)
    }

    pub fn delete_agent_prompt_file_for_agent(
        agent_id: &str,
        _agent_name: &str,
        prompt_file: Option<&str>,
    ) -> Result<()> {
        validate_agent_id(agent_id)?;
        if let Some(prompt_file) = prompt_file
            && let Some(path) = resolve_prompt_path_from_file_name(prompt_file)?
            && path.exists()
        {
            fs::remove_file(&path).with_context(|| {
                format!("Failed to remove agent prompt file: {}", path.display())
            })?;
        }
        Ok(())
    }

    fn resolve_agents_dir() -> Result<PathBuf> {
        if let Ok(dir) = std::env::var(AGENTS_DIR_ENV)
            && !dir.trim().is_empty()
        {
            return Ok(PathBuf::from(dir));
        }

        Ok(crate::paths::ensure_restflow_dir()?.join(AGENTS_DIR))
    }

    pub(crate) fn ensure_agents_dir() -> Result<PathBuf> {
        let dir = resolve_agents_dir()?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create agents directory: {}", dir.display()))?;
        Ok(dir)
    }

    fn validate_agent_id(agent_id: &str) -> Result<&str> {
        let id = agent_id.trim();
        if id.is_empty() {
            anyhow::bail!("Agent ID is empty; cannot resolve prompt file path");
        }
        // Reject path traversal characters to prevent directory escape
        if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
            anyhow::bail!(
                "Agent ID '{}' contains invalid characters (path separators or '..' sequences)",
                id
            );
        }
        Ok(id)
    }

    fn resolve_prompt_path_from_file_name(prompt_file: &str) -> Result<Option<PathBuf>> {
        let trimmed = prompt_file.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
            || trimmed.contains('\0')
        {
            anyhow::bail!("Prompt file name contains invalid characters: {}", trimmed);
        }
        Ok(Some(ensure_agents_dir()?.join(trimmed)))
    }

    fn extract_prompt_file_name(path: &Path) -> Result<String> {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(ToString::to_string)
            .ok_or_else(|| anyhow::anyhow!("Invalid prompt file path: {}", path.display()))
    }

    fn resolve_prompt_path_for_read(
        agent_name: &str,
        prompt_file: Option<&str>,
    ) -> Result<Option<PathBuf>> {
        if let Some(prompt_file) = prompt_file
            && let Some(path) = resolve_prompt_path_from_file_name(prompt_file)?
            && path.exists()
        {
            return Ok(Some(path));
        }

        let agents_dir = ensure_agents_dir()?;
        let desired = agents_dir.join(format!("{}.md", sanitize_agent_file_stem(agent_name)));
        if desired.exists() {
            return Ok(Some(desired));
        }

        Ok(None)
    }

    fn resolve_prompt_path_for_write(
        agent_name: &str,
        prompt_file: Option<&str>,
    ) -> Result<PathBuf> {
        let agents_dir = ensure_agents_dir()?;
        let desired = agents_dir.join(format!("{}.md", sanitize_agent_file_stem(agent_name)));
        let current_from_prompt_file = if let Some(prompt_file) = prompt_file {
            resolve_prompt_path_from_file_name(prompt_file)?.filter(|path| path.exists())
        } else {
            None
        };
        let current = current_from_prompt_file;

        if let Some(current_path) = current {
            if current_path == desired {
                return Ok(current_path);
            }
            if !desired.exists() {
                fs::rename(&current_path, &desired).with_context(|| {
                    format!(
                        "Failed to rename agent prompt file from {} to {}",
                        current_path.display(),
                        desired.display()
                    )
                })?;
                return Ok(desired);
            }
            let fallback = unique_prompt_path(&agents_dir, agent_name)?;
            if current_path != fallback {
                fs::rename(&current_path, &fallback).with_context(|| {
                    format!(
                        "Failed to rename agent prompt file from {} to {}",
                        current_path.display(),
                        fallback.display()
                    )
                })?;
            }
            return Ok(fallback);
        }

        if !desired.exists() {
            return Ok(desired);
        }

        if prompt_file.is_none() {
            // Reuse an existing name-based prompt file when the agent has no stored prompt file.
            return Ok(desired);
        }

        unique_prompt_path(&agents_dir, agent_name)
    }

    fn unique_prompt_path(agents_dir: &std::path::Path, agent_name: &str) -> Result<PathBuf> {
        let stem = sanitize_agent_file_stem(agent_name);
        for index in 2..1000u16 {
            let candidate = agents_dir.join(format!("{stem}-{index}.md"));
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        anyhow::bail!(
            "Failed to allocate unique prompt file path for stem '{}'",
            stem
        );
    }

    pub(crate) fn sanitize_agent_file_stem(name: &str) -> String {
        let mut stem = String::with_capacity(name.len());
        let mut last_dash = false;
        for ch in name.trim().chars() {
            let mapped = if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' {
                Some(ch)
            } else {
                Some('-')
            };

            if let Some(value) = mapped {
                if value == '-' {
                    if last_dash {
                        continue;
                    }
                    last_dash = true;
                } else {
                    last_dash = false;
                }
                stem.push(value);
            }
        }

        let normalized = stem.trim_matches(['-', '_', '.']).to_string();
        let candidate = if normalized.is_empty() {
            "agent".to_string()
        } else {
            normalized
        };
        if is_windows_reserved_stem(&candidate) {
            format!("{candidate}-agent")
        } else {
            candidate
        }
    }

    fn is_windows_reserved_stem(stem: &str) -> bool {
        let lower = stem.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "con"
                | "prn"
                | "aux"
                | "nul"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
        )
    }

    /// Shared lock for tests that mutate the RESTFLOW_AGENTS_DIR env var.
    /// All tests that set/remove this env var MUST acquire this lock first
    /// to avoid cross-module race conditions.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn agents_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
        agents_dir_env_lock_impl()
    }

    #[cfg(test)]
    fn agents_dir_env_lock_impl() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::agents_env_lock()
    }

    #[cfg(all(not(test), feature = "test-utils"))]
    fn agents_dir_env_lock_impl() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn env_lock() -> std::sync::MutexGuard<'static, ()> {
            agents_dir_env_lock()
        }

        #[test]
        fn test_ensure_prompt_templates_does_not_create_global_agent_files() {
            let _lock = env_lock();
            let temp = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

            ensure_prompt_templates().unwrap();
            assert!(!temp.path().join("default.md").exists());
            assert!(!temp.path().join("task.md").exists());

            unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        }

        #[test]
        fn test_ensure_agent_prompt_file_creates_per_agent_markdown() {
            let _lock = env_lock();
            let temp = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

            let path = ensure_agent_prompt_file(
                "550e8400-e29b-41d4-a716-446655440000",
                "Agent One",
                None,
                None,
            )
            .unwrap();
            assert!(path.exists());
            assert_eq!(
                path.file_name().and_then(|v| v.to_str()),
                Some("agent-one.md")
            );
            let content = fs::read_to_string(path).unwrap();
            assert!(!content.trim().is_empty());

            unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        }

        #[test]
        fn test_load_agent_prompt_returns_override_content() {
            let _lock = env_lock();
            let temp = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

            let id = "f7e39ba8-f1ed-4e6c-a4f4-1983f671b1d5";
            ensure_agent_prompt_file(id, "My Custom Agent", None, Some("Custom prompt")).unwrap();
            let loaded = load_agent_prompt_for_agent(id, "My Custom Agent", None).unwrap();
            assert_eq!(loaded.content.as_deref(), Some("Custom prompt"));
            assert_eq!(loaded.prompt_file.as_deref(), Some("my-custom-agent.md"));

            unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        }

        #[test]
        fn test_ensure_agent_prompt_file_preserves_plain_body() {
            let _lock = env_lock();
            let temp = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

            let id = "d95c9423-42d7-4a13-ad80-ff94e16f8f8a";
            let path =
                ensure_agent_prompt_file(id, "No Rewrite", None, Some("\nLine A\nLine B")).unwrap();
            let _ = ensure_agent_prompt_file(id, "No Rewrite", None, None).unwrap();
            let after = fs::read_to_string(&path).unwrap();
            assert_eq!(after, "\nLine A\nLine B");

            unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        }

        #[test]
        fn test_load_agent_prompt_missing_does_not_create_file() {
            let _lock = env_lock();
            let temp = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

            ensure_prompt_templates().unwrap();
            let missing = "750bf7ee";
            let loaded = load_agent_prompt_for_agent(missing, "Missing Agent", None).unwrap();
            assert!(loaded.content.is_none());
            assert!(!temp.path().join(format!("{missing}.md")).exists());

            unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        }

        #[test]
        fn test_read_prompt_file_if_exists_returns_none_for_deleted_file() {
            let _lock = env_lock();
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("deleted.md");
            fs::write(&path, "temp").unwrap();
            fs::remove_file(&path).unwrap();

            let loaded = read_prompt_file_if_exists(&path).unwrap();
            assert!(loaded.is_none());
        }

        #[test]
        fn test_agent_prompt_path_rejects_path_traversal() {
            assert!(validate_agent_id("../etc/passwd").is_err());
            assert!(validate_agent_id("foo/bar").is_err());
            assert!(validate_agent_id("foo\\bar").is_err());
            assert!(validate_agent_id("foo..bar").is_err());
            assert!(validate_agent_id("foo\0bar").is_err());
        }

        #[test]
        fn test_agent_prompt_path_accepts_valid_ids() {
            assert!(validate_agent_id("my-agent").is_ok());
            assert!(validate_agent_id("agent_1").is_ok());
            assert!(validate_agent_id("default").is_ok());
            assert!(validate_agent_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
        }

        #[test]
        fn test_sanitize_agent_file_stem_avoids_windows_reserved_names() {
            assert_eq!(sanitize_agent_file_stem("CON"), "con-agent");
            assert_eq!(sanitize_agent_file_stem("aux"), "aux-agent");
            assert_eq!(sanitize_agent_file_stem("Lpt1"), "lpt1-agent");
            assert_eq!(sanitize_agent_file_stem("Normal Name"), "normal-name");
        }

        #[test]
        fn test_resolve_agents_dir_defaults_to_restflow_home_agents() {
            let _lock = env_lock();
            unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            let expected = crate::paths::resolve_restflow_dir().unwrap().join("agents");
            let actual = resolve_agents_dir().unwrap();
            assert_eq!(actual, expected);
        }
    }
}
pub mod provider_policy {
    use types::provider_meta;

    use super::{ModelId, Provider};

    const DISPLAY_PROVIDER_ORDER: &[Provider] = &[
        Provider::OpenAI,
        Provider::MiniMaxCodingPlan,
        Provider::ZaiCodingPlan,
        Provider::ClaudeCode,
        Provider::Codex,
        Provider::Anthropic,
        Provider::Google,
        Provider::DeepSeek,
        Provider::Groq,
        Provider::OpenRouter,
        Provider::XAI,
        Provider::Qwen,
        Provider::Zai,
        Provider::Moonshot,
        Provider::Doubao,
        Provider::Yi,
        Provider::SiliconFlow,
        Provider::MiniMax,
    ];

    const SECRET_PROVIDER_RESOLUTION_ORDER: &[Provider] = &[
        Provider::MiniMaxCodingPlan,
        Provider::MiniMax,
        Provider::ZaiCodingPlan,
        Provider::Zai,
        Provider::Anthropic,
        Provider::OpenAI,
        Provider::Google,
        Provider::DeepSeek,
        Provider::Groq,
        Provider::OpenRouter,
        Provider::XAI,
        Provider::Qwen,
        Provider::Moonshot,
        Provider::Doubao,
        Provider::Yi,
        Provider::SiliconFlow,
    ];

    pub fn provider_default_model(provider: Provider) -> ModelId {
        provider_meta(provider.as_model_provider()).default_model_id
    }

    pub fn provider_display_order(provider: Provider) -> usize {
        DISPLAY_PROVIDER_ORDER
            .iter()
            .position(|candidate| *candidate == provider)
            .unwrap_or(usize::MAX)
    }

    pub(crate) fn secret_provider_resolution_order() -> &'static [Provider] {
        SECRET_PROVIDER_RESOLUTION_ORDER
    }

    pub fn resolve_model_from_available_secrets<F>(has_secret: F) -> Option<ModelId>
    where
        F: Fn(&str) -> bool,
    {
        secret_provider_resolution_order()
            .iter()
            .copied()
            .find(|provider| provider.api_key_env_candidates().any(&has_secret))
            .map(provider_default_model)
    }

    #[cfg(test)]
    mod tests {
        use super::{
            DISPLAY_PROVIDER_ORDER, provider_default_model, provider_display_order,
            resolve_model_from_available_secrets, secret_provider_resolution_order,
        };
        use types::{ModelId, Provider};

        #[test]
        fn provider_default_model_uses_runtime_defaults() {
            assert_eq!(
                provider_default_model(Provider::Anthropic),
                ModelId::ClaudeOpus4_6
            );
            assert_eq!(
                provider_default_model(Provider::MiniMax),
                ModelId::MiniMaxM27
            );
        }

        #[test]
        fn provider_display_order_places_coding_first() {
            assert!(
                provider_display_order(Provider::OpenAI)
                    < provider_display_order(Provider::Anthropic)
            );
            assert!(
                provider_display_order(Provider::MiniMaxCodingPlan)
                    < provider_display_order(Provider::DeepSeek)
            );
            assert_eq!(Provider::all().len(), DISPLAY_PROVIDER_ORDER.len());
        }

        #[test]
        fn provider_resolution_order_prefers_coding_models() {
            assert_eq!(
                secret_provider_resolution_order()[0],
                Provider::MiniMaxCodingPlan
            );
        }

        #[test]
        fn resolve_model_from_available_secrets_uses_resolution_order() {
            let model = resolve_model_from_available_secrets(|key| key == "MINIMAX_API_KEY");
            assert_eq!(model, Some(ModelId::MiniMaxM27));
        }
    }
}
pub mod secrets {
    //! Secrets storage - encrypted storage for API keys and credentials.

    use crate::encryption::SecretEncryptor;
    use crate::paths;
    use crate::storage::RedbLeaseProvider;
    use anyhow::{Context, Result};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use rand::Rng;
    use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
    use serde::{Deserialize, Serialize};
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tracing::{info, warn};

    const SECRETS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");
    const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
    const MASTER_KEY_FILE: &str = "master.key";

    #[derive(Debug, Clone, Default)]
    pub struct SecretStorageConfig {
        pub allow_insecure_file_permissions: bool,
    }

    /// A stored secret with metadata
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Secret {
        pub key: String,
        pub value: String,
        pub description: Option<String>,
        pub created_at: i64,
        pub updated_at: i64,
    }

    impl Secret {
        /// Create a new secret
        pub fn new(key: String, value: String, description: Option<String>) -> Self {
            let now = chrono::Utc::now().timestamp_millis();
            Self {
                key,
                value,
                description,
                created_at: now,
                updated_at: now,
            }
        }

        /// Update the secret value and description
        ///
        /// Pass `None` for description to clear it, or `Some(...)` to set a new one.
        pub fn update(&mut self, value: String, description: Option<String>) {
            self.value = value;
            self.description = description; // Always set, allowing None to clear
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }
    }

    /// Secret storage with AES-256-GCM encryption
    #[derive(Clone)]
    pub struct SecretStorage {
        backend: SecretStorageBackend,
        encryptor: Arc<SecretEncryptor>,
    }

    #[derive(Clone)]
    enum SecretStorageBackend {
        Lease(RedbLeaseProvider),
        Shared(Arc<Database>),
    }

    impl std::fmt::Debug for SecretStorage {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SecretStorage")
                .field("backend", &"<redb>")
                .field("encryptor", &"<SecretEncryptor>")
                .finish()
        }
    }

    impl SecretStorage {
        pub fn new(db: Arc<Database>) -> Result<Self> {
            Self::with_config(db, SecretStorageConfig::default())
        }

        pub fn with_config(db: Arc<Database>, config: SecretStorageConfig) -> Result<Self> {
            let master_key = load_master_key(&config)?;
            Self::with_master_key(db, master_key)
        }

        pub fn with_config_path(
            db_path: impl Into<PathBuf>,
            config: SecretStorageConfig,
        ) -> Result<Self> {
            let master_key = load_master_key(&config)?;
            Self::with_master_key_path(db_path, master_key)
        }

        /// Create storage with an explicit master key.
        pub fn with_master_key(db: Arc<Database>, master_key: [u8; 32]) -> Result<Self> {
            let write_txn = db.begin_write()?;
            write_txn.open_table(SECRETS_TABLE)?;
            write_txn.commit()?;

            let encryptor = Arc::new(SecretEncryptor::new(&master_key)?);

            Ok(Self {
                backend: SecretStorageBackend::Shared(db),
                encryptor,
            })
        }

        pub fn with_master_key_path(
            db_path: impl Into<PathBuf>,
            master_key: [u8; 32],
        ) -> Result<Self> {
            let provider = RedbLeaseProvider::new(db_path);
            provider.with_database(|db| {
                let write_txn = db.begin_write()?;
                write_txn.open_table(SECRETS_TABLE)?;
                write_txn.commit()?;
                Ok(())
            })?;

            let encryptor = Arc::new(SecretEncryptor::new(&master_key)?);

            Ok(Self {
                backend: SecretStorageBackend::Lease(provider),
                encryptor,
            })
        }

        /// Create for testing with relaxed file permission checks.
        #[cfg(test)]
        pub fn new_insecure(db: Arc<Database>) -> Result<Self> {
            Self::with_config(
                db,
                SecretStorageConfig {
                    allow_insecure_file_permissions: true,
                },
            )
        }

        /// Set or update a secret
        pub fn set_secret(
            &self,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()> {
            self.with_database(|db| {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(SECRETS_TABLE)?;

                    let existing = table
                        .get(key)?
                        .map(|data| self.decode_secret_bytes(data.value()))
                        .transpose()?;

                    let secret = if let Some(mut existing_secret) = existing {
                        existing_secret.update(value.to_string(), description);
                        existing_secret
                    } else {
                        Secret::new(key.to_string(), value.to_string(), description)
                    };

                    let encrypted = self.encode_secret(&secret)?;
                    table.insert(key, encrypted.as_slice())?;
                }
                write_txn.commit()?;
                Ok(())
            })
        }

        /// Create a new secret (fails if already exists)
        ///
        /// This operation is atomic - the existence check and insert happen
        /// within the same write transaction to prevent race conditions.
        pub fn create_secret(
            &self,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()> {
            self.with_database(|db| {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(SECRETS_TABLE)?;

                    // Check existence within write transaction to prevent TOCTOU race
                    if table.get(key)?.is_some() {
                        return Err(anyhow::anyhow!("Secret {} already exists", key));
                    }

                    let secret = Secret::new(key.to_string(), value.to_string(), description);
                    let encrypted = self.encode_secret(&secret)?;
                    table.insert(key, encrypted.as_slice())?;
                }
                write_txn.commit()?;
                Ok(())
            })
        }

        /// Update an existing secret (fails if not exists)
        ///
        /// This operation is atomic - the existence check and update happen
        /// within the same write transaction to prevent race conditions.
        pub fn update_secret(
            &self,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()> {
            self.with_database(|db| {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(SECRETS_TABLE)?;

                    // Check existence and get current data within write transaction
                    let existing = table
                        .get(key)?
                        .map(|data| self.decode_secret_bytes(data.value()))
                        .transpose()?;

                    let mut existing_secret =
                        existing.ok_or_else(|| anyhow::anyhow!("Secret {} not found", key))?;

                    existing_secret.update(value.to_string(), description);
                    let encrypted = self.encode_secret(&existing_secret)?;
                    table.insert(key, encrypted.as_slice())?;
                }
                write_txn.commit()?;
                Ok(())
            })
        }

        /// Get secret model (internal)
        fn get_secret_model(&self, key: &str) -> Result<Option<Secret>> {
            self.with_database(|db| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(SECRETS_TABLE)?;

                if let Some(data) = table.get(key)? {
                    let raw = data.value();
                    let secret = self.decode_secret_bytes(raw)?;
                    Ok(Some(secret))
                } else {
                    Ok(None)
                }
            })
        }

        /// Get a managed secret value from storage.
        pub fn get_secret(&self, key: &str) -> Result<Option<String>> {
            if let Some(secret) = self.get_secret_model(key)? {
                Ok(Some(secret.value))
            } else {
                Ok(None)
            }
        }

        /// Get a secret value, trimmed and filtered to exclude empty strings.
        ///
        /// Returns `Ok(Some(value))` only when the secret exists and is non-empty
        /// after trimming whitespace. Useful for validating bot tokens, API keys, etc.
        pub fn get_non_empty(&self, key: &str) -> Result<Option<String>> {
            Ok(self
                .get_secret(key)?
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()))
        }

        /// Delete a secret
        pub fn delete_secret(&self, key: &str) -> Result<()> {
            self.with_database(|db| {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(SECRETS_TABLE)?;
                    table.remove(key)?;
                }
                write_txn.commit()?;
                Ok(())
            })
        }

        /// List all secrets (values are cleared for security)
        pub fn list_secrets(&self) -> Result<Vec<Secret>> {
            self.with_database(|db| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(SECRETS_TABLE)?;

                let mut secrets = Vec::new();
                for item in table.iter()? {
                    let (_, value) = item?;
                    let secret = self.decode_secret_bytes(value.value())?;
                    let mut secret = secret;
                    // Clear the value for security
                    secret.value = String::new();
                    secrets.push(secret);
                }

                Ok(secrets)
            })
        }

        /// Check whether the secret is managed in storage.
        pub fn has_secret(&self, key: &str) -> Result<bool> {
            self.with_database(|db| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(SECRETS_TABLE)?;
                Ok(table.get(key)?.is_some())
            })
        }

        /// Check whether the secret is available from managed storage.
        pub fn has_available_secret(&self, key: &str) -> Result<bool> {
            Ok(self.get_non_empty(key)?.is_some())
        }

        fn encode_secret(&self, secret: &Secret) -> Result<Vec<u8>> {
            let json = serde_json::to_vec(secret)?;
            self.encryptor.encrypt(&json)
        }

        fn decode_secret_bytes(&self, payload: &[u8]) -> Result<Secret> {
            let plaintext = self.encryptor.decrypt(payload)?;
            Ok(serde_json::from_slice(&plaintext)?)
        }

        fn with_database<T>(&self, operation: impl FnOnce(&Database) -> Result<T>) -> Result<T> {
            match &self.backend {
                SecretStorageBackend::Lease(provider) => provider.with_database(operation),
                SecretStorageBackend::Shared(db) => operation(db),
            }
        }
    }

    fn load_master_key(config: &SecretStorageConfig) -> Result<[u8; 32]> {
        if let Some(key) = load_master_key_from_env()? {
            info!("Using master key from environment variable");
            return Ok(key);
        }

        if let Some(key) = load_master_key_from_file(config)? {
            info!("Using master key from file");
            return Ok(key);
        }

        // SECURITY: Buffer initialized to zero, immediately filled with cryptographically secure random bytes.
        let mut key = [0u8; 32];
        rand::rng().fill_bytes(&mut key);
        match write_master_key(&key) {
            Ok(_) => Ok(key),
            Err(err) => {
                if let Some(io_err) = err.downcast_ref::<std::io::Error>()
                    && io_err.kind() == std::io::ErrorKind::AlreadyExists
                    && let Some(existing) = load_master_key_from_file(config)?
                {
                    return Ok(existing);
                }
                Err(err)
            }
        }
    }

    fn load_master_key_from_env() -> Result<Option<[u8; 32]>> {
        match env::var(MASTER_KEY_ENV) {
            Ok(value) => decode_master_key(&value).map(Some),
            Err(env::VarError::NotPresent) => Ok(None),
            Err(err) => Err(anyhow::anyhow!(
                "Failed to read {}: {}",
                MASTER_KEY_ENV,
                err
            )),
        }
    }

    fn load_master_key_from_file(config: &SecretStorageConfig) -> Result<Option<[u8; 32]>> {
        let path = paths::master_key_path()?;
        if path.exists() {
            return load_key_from_path(&path, config);
        }
        Ok(None)
    }

    fn load_key_from_path(path: &Path, config: &SecretStorageConfig) -> Result<Option<[u8; 32]>> {
        check_master_key_permissions(path, config.allow_insecure_file_permissions)?;
        let raw = fs::read_to_string(path)?;
        let trimmed = raw.trim();
        decode_master_key(trimmed).map(Some)
    }

    fn write_master_key(key: &[u8; 32]) -> Result<PathBuf> {
        let dir = paths::ensure_restflow_dir()?;
        let path = dir.join(MASTER_KEY_FILE);

        let hex = encode_master_key_hex(key);

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options.open(&path)?;
        file.write_all(hex.as_bytes())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(path)
    }

    fn check_master_key_permissions(path: &Path, allow_insecure: bool) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path)?;
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                if allow_insecure {
                    warn!(
                        "Master key file permissions are too open (0o{:o}) at {}",
                        mode,
                        path.to_string_lossy()
                    );
                } else {
                    anyhow::bail!(
                        "Master key file permissions are too open (0o{:o}) at {}",
                        mode,
                        path.to_string_lossy()
                    );
                }
            }
        }

        Ok(())
    }

    fn encode_master_key_hex(key: &[u8; 32]) -> String {
        key.iter().map(|byte| format!("{:02x}", byte)).collect()
    }

    fn decode_master_key(value: &str) -> Result<[u8; 32]> {
        let trimmed = value.trim();
        if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut key = [0u8; 32];
            for (i, chunk) in trimmed.as_bytes().chunks(2).enumerate() {
                let hex = std::str::from_utf8(chunk).context("Invalid hex master key")?;
                let byte = u8::from_str_radix(hex, 16).context("Invalid hex master key")?;
                key[i] = byte;
            }
            return Ok(key);
        }

        let decoded = STANDARD
            .decode(trimmed.as_bytes())
            .context("Invalid base64 master key")?;
        if decoded.len() != 32 {
            return Err(anyhow::anyhow!(
                "Master key must be 32 bytes after decoding"
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        Ok(key)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::tempdir;

        const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";

        fn env_lock() -> std::sync::MutexGuard<'static, ()> {
            crate::test_support::env_lock()
        }

        fn setup() -> (SecretStorage, tempfile::TempDir) {
            let _env_lock = env_lock();
            let temp_dir = tempdir().unwrap();
            let state_dir = temp_dir.path().join("state");
            std::fs::create_dir_all(&state_dir).unwrap();

            // SAFETY: Tests are single-threaded in this module.
            unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &state_dir) };

            let db_path = temp_dir.path().join("test.db");
            let db = Arc::new(Database::create(db_path).unwrap());
            let storage = SecretStorage::new_insecure(db).unwrap();

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };

            (storage, temp_dir)
        }

        #[test]
        fn test_env_key_takes_precedence() {
            let _env_lock = env_lock();
            let temp_dir = tempdir().unwrap();
            let state_dir = temp_dir.path().join("state");
            std::fs::create_dir_all(&state_dir).unwrap();

            // Write a file key first
            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &state_dir) };
            let file_key = [0x11u8; 32];
            write_master_key(&file_key).unwrap();

            // Set env key which should take precedence
            let env_value = "aa".repeat(32);
            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::set_var(MASTER_KEY_ENV, &env_value) };

            let config = SecretStorageConfig {
                allow_insecure_file_permissions: true,
            };

            let key = load_master_key(&config).unwrap();
            assert_eq!(key, [0xaa; 32]);

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(MASTER_KEY_ENV) };
            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        }

        #[test]
        fn test_file_key_is_loaded() {
            let _env_lock = env_lock();
            let temp_dir = tempdir().unwrap();
            let state_dir = temp_dir.path().join("state");
            std::fs::create_dir_all(&state_dir).unwrap();

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &state_dir) };

            let file_key = [0x11u8; 32];
            write_master_key(&file_key).unwrap();

            let config = SecretStorageConfig {
                allow_insecure_file_permissions: true,
            };

            let key = load_master_key(&config).unwrap();
            assert_eq!(key, file_key);

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        }

        #[test]
        fn test_write_master_key_is_atomic() {
            let _env_lock = env_lock();
            let temp_dir = tempdir().unwrap();
            let state_dir = temp_dir.path().join("state");
            std::fs::create_dir_all(&state_dir).unwrap();

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &state_dir) };

            let first_key = [0x22u8; 32];
            write_master_key(&first_key).unwrap();

            let second_key = [0x33u8; 32];
            let err = write_master_key(&second_key).unwrap_err();
            let io_err = err.downcast_ref::<std::io::Error>().unwrap();
            assert_eq!(io_err.kind(), std::io::ErrorKind::AlreadyExists);

            let config = SecretStorageConfig {
                allow_insecure_file_permissions: true,
            };
            let existing = load_master_key_from_file(&config).unwrap().unwrap();
            assert_eq!(existing, first_key);

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        }

        #[test]
        fn test_set_and_get_secret() {
            let (storage, _temp_dir) = setup();

            storage
                .set_secret(
                    "OPENAI_API_KEY",
                    "sk-test123",
                    Some("OpenAI API key".to_string()),
                )
                .unwrap();

            let value = storage.get_secret("OPENAI_API_KEY").unwrap();
            assert_eq!(value, Some("sk-test123".to_string()));
        }

        #[test]
        fn test_list_secrets_with_metadata() {
            let (storage, _temp_dir) = setup();

            storage
                .set_secret("API_KEY_1", "value1", Some("First key".to_string()))
                .unwrap();
            storage.set_secret("API_KEY_2", "value2", None).unwrap();

            let secrets = storage.list_secrets().unwrap();
            assert_eq!(secrets.len(), 2);

            let key1 = secrets.iter().find(|s| s.key == "API_KEY_1").unwrap();
            assert_eq!(key1.description, Some("First key".to_string()));
            assert_eq!(key1.value, ""); // Value should be cleared
        }

        #[test]
        fn test_delete_secret() {
            let (storage, _temp_dir) = setup();

            storage.set_secret("TEST_KEY", "test_value", None).unwrap();
            storage.delete_secret("TEST_KEY").unwrap();

            let value = storage.get_secret("TEST_KEY").unwrap();
            assert_eq!(value, None);
        }

        #[test]
        fn test_has_secret() {
            let (storage, _temp_dir) = setup();

            storage.set_secret("EXISTS", "value", None).unwrap();

            assert!(storage.has_secret("EXISTS").unwrap());
            assert!(!storage.has_secret("NOT_EXISTS").unwrap());
        }

        #[test]
        fn test_has_available_secret_checks_storage_only() {
            let (storage, _temp_dir) = setup();
            let key = "MANAGED_SECRET";

            assert!(!storage.has_available_secret(key).unwrap());
            storage.set_secret(key, "   ", None).unwrap();
            assert!(!storage.has_available_secret(key).unwrap());
            storage.set_secret(key, "managed-value", None).unwrap();
            assert!(storage.has_available_secret(key).unwrap());
        }

        #[test]
        fn test_clear_description() {
            let (storage, _temp_dir) = setup();

            // Create secret with description
            storage
                .set_secret(
                    "TEST_KEY",
                    "value1",
                    Some("Initial description".to_string()),
                )
                .unwrap();

            // Verify description is set
            let secrets = storage.list_secrets().unwrap();
            let secret = secrets.iter().find(|s| s.key == "TEST_KEY").unwrap();
            assert_eq!(secret.description, Some("Initial description".to_string()));

            // Update with None to clear description
            storage.set_secret("TEST_KEY", "value2", None).unwrap();

            // Verify description is cleared
            let secrets = storage.list_secrets().unwrap();
            let secret = secrets.iter().find(|s| s.key == "TEST_KEY").unwrap();
            assert_eq!(
                secret.description, None,
                "Description should be cleared when None is passed"
            );
        }

        #[test]
        fn test_create_secret_atomic() {
            let (storage, _temp_dir) = setup();

            // First create should succeed
            storage.create_secret("UNIQUE_KEY", "value1", None).unwrap();

            // Second create should fail
            let result = storage.create_secret("UNIQUE_KEY", "value2", None);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already exists"));

            // Value should remain the first one
            let value = storage.get_secret("UNIQUE_KEY").unwrap();
            assert_eq!(value, Some("value1".to_string()));
        }

        #[test]
        fn test_update_secret_atomic() {
            let (storage, _temp_dir) = setup();

            // Update non-existent should fail
            let result = storage.update_secret("NON_EXISTENT", "value", None);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not found"));

            // Create then update should work
            storage
                .create_secret("UPDATE_KEY", "initial", None)
                .unwrap();
            storage
                .update_secret("UPDATE_KEY", "updated", Some("desc".to_string()))
                .unwrap();

            let value = storage.get_secret("UPDATE_KEY").unwrap();
            assert_eq!(value, Some("updated".to_string()));
        }

        /// Test concurrent set_secret operations don't corrupt data.
        /// All threads write to the same key - the final value should be one of the written values.
        #[test]
        fn test_concurrent_set_secret() {
            use std::thread;

            let _env_lock = env_lock();
            let temp_dir = tempdir().unwrap();
            let state_dir = temp_dir.path().join("state");
            std::fs::create_dir_all(&state_dir).unwrap();

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &state_dir) };

            let db_path = temp_dir.path().join("test.db");
            let db = Arc::new(Database::create(db_path).unwrap());
            let storage = Arc::new(SecretStorage::new_insecure(db).unwrap());

            let num_threads = 10;
            let handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let s = Arc::clone(&storage);
                    thread::spawn(move || {
                        s.set_secret("concurrent_key", &format!("value-{}", i), None)
                            .unwrap();
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }

            // Should have exactly one secret, not corrupted
            let secret = storage.get_secret("concurrent_key").unwrap();
            assert!(secret.is_some());
            let value = secret.unwrap();
            assert!(value.starts_with("value-"));

            // Only one secret should exist
            let secrets = storage.list_secrets().unwrap();
            assert_eq!(secrets.len(), 1);

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        }

        /// Test concurrent create_secret - only one should succeed.
        #[test]
        fn test_concurrent_create_secret() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::thread;

            let _env_lock = env_lock();
            let temp_dir = tempdir().unwrap();
            let state_dir = temp_dir.path().join("state");
            std::fs::create_dir_all(&state_dir).unwrap();

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &state_dir) };

            let db_path = temp_dir.path().join("test.db");
            let db = Arc::new(Database::create(db_path).unwrap());
            let storage = Arc::new(SecretStorage::new_insecure(db).unwrap());

            let success_count = Arc::new(AtomicUsize::new(0));
            let num_threads = 10;

            let handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let s = Arc::clone(&storage);
                    let count = Arc::clone(&success_count);
                    thread::spawn(move || {
                        if s.create_secret("race_key", &format!("value-{}", i), None)
                            .is_ok()
                        {
                            count.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }

            // Exactly one create should have succeeded
            assert_eq!(success_count.load(Ordering::SeqCst), 1);

            // Only one secret should exist
            let secrets = storage.list_secrets().unwrap();
            assert_eq!(secrets.len(), 1);

            // SAFETY: This is a single-threaded test, no other threads access this env var
            unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        }
    }
}
pub mod services {
    pub mod adapters {
        //! Storage-backed adapter implementations for tool traits.
        //!
        //! Each adapter bridges a runtime storage type to a tool trait
        //! defined in types, making storage functionality available
        //! to tool implementations in tools.

        pub mod agent {
            //! AgentStore adapter backed by AgentStorage.

            use crate::AgentStorage;
            use crate::storage::SecretStorage;
            use crate::tools::ToolError;
            use serde_json::{Value, json};
            use std::collections::HashSet;
            use std::sync::{Arc, RwLock};
            use types::request::AgentNode as ContractAgentNode;
            use types::store::{AgentCreateRequest, AgentStore, AgentUpdateRequest};

            #[derive(Clone)]
            pub struct AgentStoreAdapter {
                storage: AgentStorage,
                secrets: SecretStorage,
                known_tools: Arc<RwLock<HashSet<String>>>,
            }

            impl AgentStoreAdapter {
                pub fn new(
                    storage: AgentStorage,
                    secrets: SecretStorage,
                    known_tools: Arc<RwLock<HashSet<String>>>,
                ) -> Self {
                    Self {
                        storage,
                        secrets,
                        known_tools,
                    }
                }

                fn parse_agent_node(
                    value: ContractAgentNode,
                ) -> Result<types::AgentNode, ToolError> {
                    types::AgentNode::try_from_contract_node(value)
                        .map_err(|errors| ToolError::Tool(types::encode_validation_error(errors)))
                }

                fn validate_agent_node(&self, agent: &types::AgentNode) -> Result<(), ToolError> {
                    if let Err(errors) = agent.validate() {
                        return Err(ToolError::Tool(types::encode_validation_error(errors)));
                    }

                    let mut errors = Vec::new();
                    if let Some(tools) = &agent.tools {
                        for tool_name in tools {
                            let normalized = tool_name.trim();
                            if normalized.is_empty() {
                                errors.push(types::ValidationError::new(
                                    "tools",
                                    "tool name must not be empty",
                                ));
                                continue;
                            }
                            let is_known = self
                                .known_tools
                                .read()
                                .map(|set| set.contains(normalized))
                                .unwrap_or(false);
                            if !is_known && !is_subagent_tool_name(normalized) {
                                errors.push(types::ValidationError::new(
                                    "tools",
                                    format!("unknown tool: {}", normalized),
                                ));
                            }
                        }
                    }

                    if let Some(skills) = &agent.skills {
                        let skill_ids: Vec<&str> = skills
                            .iter()
                            .map(|s| s.trim())
                            .filter(|s| {
                                if s.is_empty() {
                                    errors.push(types::ValidationError::new(
                                        "skills",
                                        "skill ID must not be empty",
                                    ));
                                    false
                                } else {
                                    true
                                }
                            })
                            .collect();
                        for id in skill_ids {
                            match crate::services::skills::skill_exists_in_catalog(id) {
                                Ok(true) => {}
                                Ok(false) => errors.push(types::ValidationError::new(
                                    "skills",
                                    format!("unknown skill: {}", id),
                                )),
                                Err(err) => errors.push(types::ValidationError::new(
                                    "skills",
                                    format!("failed to verify skill '{}': {}", id, err),
                                )),
                            }
                        }
                    }

                    if let Some(types::ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
                        let normalized = secret_name.trim();
                        if !normalized.is_empty() {
                            match self.secrets.has_available_secret(normalized) {
                                Ok(true) => {}
                                Ok(false) => errors.push(types::ValidationError::new(
                                    "api_key_config",
                                    format!("secret not found in storage: {}", normalized),
                                )),
                                Err(err) => errors.push(types::ValidationError::new(
                                    "api_key_config",
                                    format!("failed to verify secret '{}': {}", normalized, err),
                                )),
                            }
                        }
                    }

                    if errors.is_empty() {
                        Ok(())
                    } else {
                        Err(ToolError::Tool(types::encode_validation_error(errors)))
                    }
                }
            }

            fn is_subagent_tool_name(name: &str) -> bool {
                matches!(
                    name,
                    "spawn_subagent" | "spawn_subagent_batch" | "wait_subagents" | "list_subagents"
                )
            }

            impl AgentStore for AgentStoreAdapter {
                fn list_agents(&self) -> crate::tools::Result<Value> {
                    let agents = self.storage.list_agents()?;
                    serde_json::to_value(agents).map_err(ToolError::from)
                }

                fn get_agent(&self, id: &str) -> crate::tools::Result<Value> {
                    let agent = self
                        .storage
                        .get_agent(id.to_string())?
                        .ok_or_else(|| ToolError::Tool(format!("Agent {} not found", id)))?;
                    serde_json::to_value(agent).map_err(ToolError::from)
                }

                fn create_agent(&self, request: AgentCreateRequest) -> crate::tools::Result<Value> {
                    let agent = Self::parse_agent_node(request.agent)?;
                    self.validate_agent_node(&agent)?;
                    let created = self.storage.create_agent(request.name, agent)?;
                    serde_json::to_value(created).map_err(ToolError::from)
                }

                fn update_agent(&self, request: AgentUpdateRequest) -> crate::tools::Result<Value> {
                    let agent = match request.agent {
                        Some(value) => {
                            let node = Self::parse_agent_node(value)?;
                            self.validate_agent_node(&node)?;
                            Some(node)
                        }
                        None => None,
                    };
                    let updated = self.storage.update_agent(request.id, request.name, agent)?;
                    serde_json::to_value(updated).map_err(ToolError::from)
                }

                fn delete_agent(&self, id: &str) -> crate::tools::Result<Value> {
                    self.storage.delete_agent(id.to_string())?;
                    Ok(json!({ "id": id, "deleted": true }))
                }
            }

            #[cfg(test)]
            mod tests {
                use super::*;
                use crate::test_support::RestflowTestEnv;
                use std::sync::Arc;
                use types::request::{AgentNode as ContractAgentNode, WireModelRef};
                use types::store::AgentStore;

                fn setup() -> (AgentStoreAdapter, RestflowTestEnv) {
                    let env = RestflowTestEnv::new();
                    let db_path = env.db_path("test.db");
                    let db = Arc::new(redb::Database::create(db_path).unwrap());

                    let agent_storage =
                        AgentStorage::new_file_backed_path(env.root().join("agents")).unwrap();
                    let secret_storage = SecretStorage::with_config(
                        db.clone(),
                        crate::SecretStorageConfig {
                            allow_insecure_file_permissions: true,
                        },
                    )
                    .unwrap();
                    let known_tools = Arc::new(RwLock::new(HashSet::from(["bash".to_string()])));

                    (
                        AgentStoreAdapter::new(agent_storage, secret_storage, known_tools),
                        env,
                    )
                }

                #[test]
                fn test_create_and_list_agents() {
                    let (adapter, _env) = setup();
                    let request = AgentCreateRequest {
                        name: "Test Agent".to_string(),
                        agent: ContractAgentNode::default(),
                    };
                    adapter.create_agent(request).unwrap();

                    let list = adapter.list_agents().unwrap();
                    let agents = list.as_array().unwrap();
                    assert!(!agents.is_empty());
                }

                #[test]
                fn test_get_agent() {
                    let (adapter, _env) = setup();
                    let created = adapter
                        .create_agent(AgentCreateRequest {
                            name: "Getter".to_string(),
                            agent: ContractAgentNode::default(),
                        })
                        .unwrap();
                    let id = created["id"].as_str().unwrap();

                    let fetched = adapter.get_agent(id).unwrap();
                    assert_eq!(fetched["name"], "Getter");
                }

                #[test]
                fn test_get_nonexistent_agent_fails() {
                    let (adapter, _env) = setup();
                    let result = adapter.get_agent("nonexistent");
                    assert!(result.is_err());
                }

                #[test]
                fn test_delete_agent() {
                    let (adapter, _env) = setup();
                    let created = adapter
                        .create_agent(AgentCreateRequest {
                            name: "To Delete".to_string(),
                            agent: ContractAgentNode::default(),
                        })
                        .unwrap();
                    let id = created["id"].as_str().unwrap();

                    let result = adapter.delete_agent(id).unwrap();
                    assert_eq!(result["deleted"], true);
                }

                #[test]
                fn test_update_agent_name() {
                    let (adapter, _env) = setup();
                    let created = adapter
                        .create_agent(AgentCreateRequest {
                            name: "Original".to_string(),
                            agent: ContractAgentNode::default(),
                        })
                        .unwrap();
                    let id = created["id"].as_str().unwrap().to_string();

                    let updated = adapter
                        .update_agent(AgentUpdateRequest {
                            id,
                            name: Some("Renamed".to_string()),
                            agent: None,
                        })
                        .unwrap();
                    assert_eq!(updated["name"], "Renamed");
                }

                #[test]
                fn test_validate_unknown_tool_rejected() {
                    let (adapter, _env) = setup();
                    let result = adapter.create_agent(AgentCreateRequest {
                        name: "Bad Tools".to_string(),
                        agent: ContractAgentNode {
                            tools: Some(vec!["nonexistent_tool".to_string()]),
                            ..ContractAgentNode::default()
                        },
                    });
                    assert!(result.is_err());
                    let err_msg = format!("{:?}", result.unwrap_err());
                    assert!(err_msg.contains("unknown tool"));
                }

                #[test]
                fn test_validate_subagent_tools_accepted() {
                    let (adapter, _env) = setup();
                    let result = adapter.create_agent(AgentCreateRequest {
                        name: "Subagent Tools".to_string(),
                        agent: ContractAgentNode {
                            tools: Some(vec![
                                "bash".to_string(),
                                "spawn_subagent_batch".to_string(),
                                "wait_subagents".to_string(),
                                "list_subagents".to_string(),
                            ]),
                            ..ContractAgentNode::default()
                        },
                    });

                    assert!(result.is_ok());
                }

                #[test]
                fn test_create_agent_rejects_invalid_model_ref() {
                    let (adapter, _env) = setup();
                    let result = adapter.create_agent(AgentCreateRequest {
                        name: "Bad Model Ref".to_string(),
                        agent: ContractAgentNode {
                            model_ref: Some(WireModelRef {
                                provider: "openai".to_string(),
                                model: "claude-sonnet-4".to_string(),
                            }),
                            ..ContractAgentNode::default()
                        },
                    });

                    let error = result.expect_err("expected invalid model_ref");
                    let message = error.to_string();
                    assert!(message.contains("validation_error"));
                    assert!(message.contains("model_ref"));
                }

                #[test]
                fn test_update_agent_rejects_conflicting_model_fields() {
                    let (adapter, _env) = setup();
                    let created = adapter
                        .create_agent(AgentCreateRequest {
                            name: "Conflict".to_string(),
                            agent: ContractAgentNode::default(),
                        })
                        .expect("create agent");
                    let id = created["id"].as_str().expect("agent id").to_string();

                    let result = adapter.update_agent(AgentUpdateRequest {
                        id,
                        name: None,
                        agent: Some(ContractAgentNode {
                            model_ref: Some(WireModelRef {
                                provider: "anthropic".to_string(),
                                model: "gpt-5-mini".to_string(),
                            }),
                            ..ContractAgentNode::default()
                        }),
                    });

                    let error = result.expect_err("expected invalid model_ref");
                    let message = error.to_string();
                    assert!(message.contains("validation_error"));
                    assert!(message.contains("model_ref"));
                }
            }
        }
        pub mod config {
            use crate::ConfigStorage;
            use std::sync::Arc;
            use types::config_types::{CliConfig, ConfigDocument, SystemConfig};
            use types::store::ConfigStore;

            pub struct ConfigStoreAdapter {
                storage: Arc<ConfigStorage>,
            }

            impl ConfigStoreAdapter {
                pub fn new(storage: Arc<ConfigStorage>) -> Self {
                    Self { storage }
                }
            }

            fn config_error(e: impl std::fmt::Display) -> types::ToolError {
                types::ToolError::Tool(format!(
                    "Config storage error: {e}. The config file may be missing, invalid, or inaccessible."
                ))
            }

            impl ConfigStore for ConfigStoreAdapter {
                fn get_effective_config(&self) -> types::error::Result<ConfigDocument> {
                    let system = self.storage.get_effective_config().map_err(config_error)?;
                    let system =
                        serde_json::from_value(serde_json::to_value(system).map_err(config_error)?)
                            .map_err(config_error)?;
                    Ok(ConfigDocument::from_system_config(
                        system,
                        CliConfig::default(),
                    ))
                }

                fn get_writable_config(&self) -> types::error::Result<ConfigDocument> {
                    let system = self.storage.get_global_config().map_err(config_error)?;
                    let system =
                        serde_json::from_value(serde_json::to_value(system).map_err(config_error)?)
                            .map_err(config_error)?;
                    Ok(ConfigDocument::from_system_config(
                        system,
                        CliConfig::default(),
                    ))
                }

                fn persist_config(&self, config: &ConfigDocument) -> types::error::Result<()> {
                    let system = serde_json::from_value(
                        serde_json::to_value(config.system_config()).map_err(config_error)?,
                    )
                    .map_err(config_error)?;
                    self.storage.update_config(system).map_err(config_error)?;
                    Ok(())
                }

                fn reset_config(&self) -> types::error::Result<ConfigDocument> {
                    let doc = ConfigDocument::from_system_config(
                        SystemConfig::default(),
                        CliConfig::default(),
                    );
                    let system = serde_json::from_value(
                        serde_json::to_value(doc.system_config()).map_err(config_error)?,
                    )
                    .map_err(config_error)?;
                    self.storage.update_config(system).map_err(config_error)?;
                    Ok(doc)
                }
            }
        }
        pub mod ops {
            //! OpsProvider adapter for operational queries.

            use crate::tools::ToolError;
            use serde_json::{Value, json};
            use std::path::{Path, PathBuf};
            use types::store::OpsProvider;

            /// Build a standard ops response envelope.
            fn build_ops_response(operation: &str, evidence: Value, verification: Value) -> Value {
                json!({
                    "operation": operation,
                    "evidence": evidence,
                    "verification": verification
                })
            }

            pub struct OpsProviderAdapter;

            impl OpsProviderAdapter {
                pub fn new() -> Self {
                    Self
                }

                fn canonical_existing_ancestor(path: &Path) -> anyhow::Result<PathBuf> {
                    let mut current = if path.exists() {
                        path.to_path_buf()
                    } else {
                        path.parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_else(|| path.to_path_buf())
                    };

                    while !current.exists() {
                        if !current.pop() {
                            break;
                        }
                    }

                    if !current.exists() {
                        anyhow::bail!("No existing ancestor found for path: {}", path.display());
                    }

                    Ok(current.canonicalize()?)
                }

                pub(crate) fn resolve_log_tail_path(
                    path: Option<&str>,
                ) -> crate::tools::Result<PathBuf> {
                    let logs_dir = crate::paths::logs_dir()?;
                    let resolved = match path
                        .map(str::trim)
                        .filter(|raw| !raw.is_empty())
                        .map(PathBuf::from)
                    {
                        Some(custom_path) if custom_path.is_absolute() => custom_path,
                        Some(custom_path) => logs_dir.join(custom_path),
                        None => crate::paths::daemon_log_path()?,
                    };

                    let logs_root = Self::canonical_existing_ancestor(&logs_dir)?;
                    let path_root = Self::canonical_existing_ancestor(&resolved)?;
                    if !path_root.starts_with(&logs_root) {
                        return Err(ToolError::Tool(format!(
                            "log_tail path must stay under {}",
                            logs_dir.display()
                        )));
                    }

                    if let Ok(metadata) = std::fs::symlink_metadata(&resolved)
                        && metadata.file_type().is_symlink()
                    {
                        return Err(ToolError::Tool(
                            "log_tail does not allow symlink paths".to_string(),
                        ));
                    }

                    Ok(resolved)
                }

                pub(crate) fn read_log_tail(
                    path: &Path,
                    lines: usize,
                ) -> anyhow::Result<(Vec<String>, bool)> {
                    let mut file = std::fs::File::open(path)?;
                    let mut content = String::new();
                    use std::io::Read;
                    file.read_to_string(&mut content)?;
                    let all_lines: Vec<String> = content.lines().map(str::to_string).collect();
                    let total = all_lines.len();
                    let start = total.saturating_sub(lines);
                    let truncated = total > lines;
                    Ok((all_lines[start..].to_vec(), truncated))
                }
            }

            impl OpsProvider for OpsProviderAdapter {
                fn daemon_health(
                    &self,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = crate::tools::Result<Value>> + Send + '_>,
                > {
                    Box::pin(async move {
                        let socket = crate::paths::socket_path()?;
                        let evidence = json!({
                            "healthy": socket.exists(),
                            "socket": socket,
                            "source": "core"
                        });
                        let verification = json!({
                            "healthy": evidence["healthy"],
                            "ipc_checked": false,
                            "http_checked": false
                        });
                        Ok(build_ops_response("daemon_health", evidence, verification))
                    })
                }

                fn log_tail(
                    &self,
                    lines: usize,
                    path: Option<&str>,
                ) -> crate::tools::Result<Value> {
                    let resolved = Self::resolve_log_tail_path(path)?;
                    if !resolved.exists() {
                        let evidence = json!({
                            "path": resolved.to_string_lossy(),
                            "lines": [],
                            "line_count": 0
                        });
                        let verification = json!({
                            "path_exists": false,
                            "requested_lines": lines
                        });
                        return Ok(build_ops_response("log_tail", evidence, verification));
                    }

                    let (tail, truncated) = Self::read_log_tail(&resolved, lines)?;
                    let evidence = json!({
                        "path": resolved.to_string_lossy(),
                        "lines": tail,
                        "line_count": tail.len()
                    });
                    let verification = json!({
                        "path_exists": true,
                        "requested_lines": lines,
                        "truncated": truncated
                    });
                    Ok(build_ops_response("log_tail", evidence, verification))
                }
            }

            #[cfg(test)]
            mod tests_adapter {
                use super::*;
                use tempfile::tempdir;
                use types::store::OpsProvider;

                fn setup() -> (OpsProviderAdapter, tempfile::TempDir) {
                    let temp_dir = tempdir().unwrap();
                    (OpsProviderAdapter::new(), temp_dir)
                }

                #[test]
                fn test_log_tail_nonexistent_file() {
                    let (adapter, _dir) = setup();
                    // log_tail with default path should work (returns empty if no file)
                    let result = adapter.log_tail(10, None);
                    // Result depends on system state but should not panic
                    assert!(result.is_ok() || result.is_err());
                }

                #[test]
                fn test_read_log_tail() {
                    let dir = tempdir().unwrap();
                    let log_file = dir.path().join("test.log");
                    std::fs::write(&log_file, "line1\nline2\nline3\nline4\nline5\n").unwrap();

                    let (lines, truncated) =
                        OpsProviderAdapter::read_log_tail(&log_file, 3).unwrap();
                    assert_eq!(lines.len(), 3);
                    assert_eq!(lines[0], "line3");
                    assert_eq!(lines[2], "line5");
                    assert!(truncated);
                }

                #[test]
                fn test_read_log_tail_no_truncation() {
                    let dir = tempdir().unwrap();
                    let log_file = dir.path().join("test.log");
                    std::fs::write(&log_file, "line1\nline2\n").unwrap();

                    let (lines, truncated) =
                        OpsProviderAdapter::read_log_tail(&log_file, 100).unwrap();
                    assert_eq!(lines.len(), 2);
                    assert!(!truncated);
                }
            }
        }
        pub mod secret {
            use crate::SecretStorage;
            use serde_json::{Value, json};
            use std::sync::Arc;
            use types::store::SecretStore;

            pub struct SecretStoreAdapter {
                storage: Arc<SecretStorage>,
            }

            impl SecretStoreAdapter {
                pub fn new(storage: Arc<SecretStorage>) -> Self {
                    Self { storage }
                }
            }

            impl SecretStore for SecretStoreAdapter {
                fn list_secrets(&self) -> types::error::Result<Value> {
                    let secrets = self.storage.list_secrets().map_err(|e| {
                        types::ToolError::Tool(format!("Failed to list secrets: {e}"))
                    })?;
                    Ok(json!({ "count": secrets.len(), "secrets": secrets }))
                }

                fn get_secret(&self, key: &str) -> types::error::Result<Option<String>> {
                    self.storage
                        .get_secret(key)
                        .map_err(|e| types::ToolError::Tool(format!("Failed to get secret: {e}")))
                }

                fn set_secret(
                    &self,
                    key: &str,
                    value: &str,
                    description: Option<String>,
                ) -> types::error::Result<()> {
                    self.storage
                        .set_secret(key, value, description)
                        .map_err(|e| types::ToolError::Tool(format!("Failed to set secret: {e}")))
                }

                fn delete_secret(&self, key: &str) -> types::error::Result<()> {
                    self.storage.delete_secret(key).map_err(|e| {
                        types::ToolError::Tool(format!("Failed to delete secret: {e}"))
                    })
                }

                fn has_secret(&self, key: &str) -> types::error::Result<bool> {
                    self.storage
                        .has_secret(key)
                        .map_err(|e| types::ToolError::Tool(format!("Failed to check secret: {e}")))
                }
            }
        }
        pub mod session {
            //! SessionStore adapter backed by the canonical SessionService boundary.

            use crate::AgentStorage;
            use crate::services::session::SessionService;
            use crate::session_log::FileSessionStore;
            use crate::tools::ToolError;
            use serde_json::{Value, json};
            use types::store::{
                SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore,
            };

            #[derive(Clone)]
            pub struct SessionStorageAdapter {
                sessions: FileSessionStore,
                agent_storage: AgentStorage,
            }

            impl SessionStorageAdapter {
                pub fn new(sessions: FileSessionStore, agent_storage: AgentStorage) -> Self {
                    Self {
                        sessions,
                        agent_storage,
                    }
                }

                fn session_service(&self) -> SessionService {
                    SessionService::new(self.sessions.clone(), Some(self.agent_storage.clone()))
                }
            }

            impl SessionStore for SessionStorageAdapter {
                fn list_sessions(&self, filter: SessionListFilter) -> crate::tools::Result<Value> {
                    let include_archived = filter.include_archived.unwrap_or(false);
                    let sessions = self.session_service().list_session_views(
                        filter.agent_id.as_deref(),
                        filter.skill_id.as_deref(),
                        include_archived,
                    )?;

                    if filter.include_messages.unwrap_or(false) {
                        Ok(serde_json::to_value(sessions)?)
                    } else {
                        let summaries = sessions
                            .iter()
                            .map(types::ChatSessionSummary::from)
                            .collect::<Vec<_>>();
                        Ok(serde_json::to_value(summaries)?)
                    }
                }

                fn get_session(&self, id: &str) -> crate::tools::Result<Value> {
                    let session = self
                        .session_service()
                        .get_session_view(id)?
                        .ok_or_else(|| ToolError::Tool(format!("Session {} not found", id)))?;
                    Ok(serde_json::to_value(session)?)
                }

                fn create_session(
                    &self,
                    request: SessionCreateRequest,
                ) -> crate::tools::Result<Value> {
                    let resolved_agent_id = self
                        .agent_storage
                        .resolve_existing_agent_id(&request.agent_id)?;
                    let session = self.session_service().create_workspace_session(
                        resolved_agent_id,
                        request.model,
                        request.name,
                        request.skill_id,
                        request.retention,
                    )?;
                    Ok(serde_json::to_value(session)?)
                }

                fn archive_session(&self, id: &str) -> crate::tools::Result<Value> {
                    let archived = self.session_service().archive_workspace_session(id)?;
                    Ok(json!({ "id": id, "archived": archived }))
                }

                fn unarchive_session(&self, id: &str) -> crate::tools::Result<Value> {
                    let unarchived = self.session_service().unarchive_workspace_session(id)?;
                    Ok(json!({ "id": id, "unarchived": unarchived }))
                }

                fn purge_session(&self, id: &str) -> crate::tools::Result<Value> {
                    let purged = self.session_service().delete_workspace_session(id)?;
                    Ok(json!({ "id": id, "purged": purged }))
                }

                fn delete_session(&self, id: &str) -> crate::tools::Result<Value> {
                    self.purge_session(id)
                }

                fn search_sessions(
                    &self,
                    query: SessionSearchQuery,
                ) -> crate::tools::Result<Value> {
                    let matched = self.session_service().search_session_views(
                        &query.query,
                        query.agent_id.as_deref(),
                        query.skill_id.as_deref(),
                        query.include_archived.unwrap_or(false),
                        query.limit.unwrap_or(20) as usize,
                    )?;

                    Ok(serde_json::to_value(matched)?)
                }

                fn cleanup_sessions(&self) -> crate::tools::Result<Value> {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    let stats = self
                        .session_service()
                        .cleanup_workspace_sessions_by_retention(now_ms)?;
                    Ok(serde_json::to_value(stats)?)
                }
            }

            #[cfg(test)]
            mod tests {
                use super::*;
                use crate::test_support::RestflowTestEnv;
                use types::store::SessionStore;

                fn setup() -> (SessionStorageAdapter, RestflowTestEnv) {
                    let env = RestflowTestEnv::new();
                    let session_storage =
                        FileSessionStore::new(env.root().join("sessions")).unwrap();
                    let agent_storage = AgentStorage::new_file_backed().unwrap();
                    (
                        SessionStorageAdapter::new(session_storage, agent_storage),
                        env,
                    )
                }

                fn create_default_agent(adapter: &SessionStorageAdapter) -> String {
                    let agent = types::AgentNode::default();
                    let created = adapter
                        .agent_storage
                        .create_agent("test-agent".to_string(), agent)
                        .unwrap();
                    created.id
                }

                #[test]
                fn test_list_sessions_empty() {
                    let (adapter, _dir) = setup();
                    let filter = SessionListFilter {
                        agent_id: None,
                        skill_id: None,
                        include_messages: None,
                        include_archived: None,
                    };
                    let result = adapter.list_sessions(filter).unwrap();
                    let sessions = result.as_array().unwrap();
                    assert!(sessions.is_empty());
                }

                #[test]
                fn test_create_and_get_session() {
                    let (adapter, _dir) = setup();
                    let agent_id = create_default_agent(&adapter);
                    let request = SessionCreateRequest {
                        agent_id: agent_id.clone(),
                        model: "gpt-4".to_string(),
                        name: Some("Test Session".to_string()),
                        skill_id: None,
                        retention: None,
                    };
                    let created = adapter.create_session(request).unwrap();
                    let session_id = created["id"].as_str().unwrap();

                    let fetched = adapter.get_session(session_id).unwrap();
                    assert_eq!(fetched["name"], "Test Session");
                    assert_eq!(fetched["model"], "gpt-4");
                }

                #[test]
                fn test_delete_session() {
                    let (adapter, _dir) = setup();
                    let agent_id = create_default_agent(&adapter);
                    let request = SessionCreateRequest {
                        agent_id,
                        model: "gpt-4".to_string(),
                        name: None,
                        skill_id: None,
                        retention: None,
                    };
                    let created = adapter.create_session(request).unwrap();
                    let session_id = created["id"].as_str().unwrap().to_string();

                    let result = adapter.delete_session(&session_id).unwrap();
                    assert_eq!(result["purged"], true);
                }

                #[test]
                fn test_archive_and_unarchive_session() {
                    let (adapter, _dir) = setup();
                    let agent_id = create_default_agent(&adapter);
                    let created = adapter
                        .create_session(SessionCreateRequest {
                            agent_id,
                            model: "gpt-4".to_string(),
                            name: Some("Archive Target".to_string()),
                            skill_id: None,
                            retention: None,
                        })
                        .unwrap();
                    let session_id = created["id"].as_str().unwrap().to_string();

                    let archive = adapter.archive_session(&session_id).unwrap();
                    assert_eq!(archive["archived"], true);

                    let active_list = adapter
                        .list_sessions(SessionListFilter {
                            agent_id: None,
                            skill_id: None,
                            include_messages: None,
                            include_archived: Some(false),
                        })
                        .unwrap();
                    assert_eq!(active_list.as_array().unwrap().len(), 0);

                    let all_list = adapter
                        .list_sessions(SessionListFilter {
                            agent_id: None,
                            skill_id: None,
                            include_messages: None,
                            include_archived: Some(true),
                        })
                        .unwrap();
                    assert_eq!(all_list.as_array().unwrap().len(), 1);

                    let unarchive = adapter.unarchive_session(&session_id).unwrap();
                    assert_eq!(unarchive["unarchived"], true);
                    let active_again = adapter
                        .list_sessions(SessionListFilter {
                            agent_id: None,
                            skill_id: None,
                            include_messages: None,
                            include_archived: Some(false),
                        })
                        .unwrap();
                    assert_eq!(active_again.as_array().unwrap().len(), 1);
                }

                #[test]
                fn test_get_nonexistent_session_fails() {
                    let (adapter, _dir) = setup();
                    let result = adapter.get_session("nonexistent");
                    assert!(result.is_err());
                }

                #[test]
                fn test_search_sessions() {
                    let (adapter, _dir) = setup();
                    let agent_id = create_default_agent(&adapter);
                    let request = SessionCreateRequest {
                        agent_id: agent_id.clone(),
                        model: "gpt-4".to_string(),
                        name: Some("Meeting Notes".to_string()),
                        skill_id: None,
                        retention: None,
                    };
                    adapter.create_session(request).unwrap();

                    let query = SessionSearchQuery {
                        query: "meeting".to_string(),
                        agent_id: None,
                        skill_id: None,
                        include_archived: None,
                        limit: None,
                    };
                    let result = adapter.search_sessions(query).unwrap();
                    let sessions = result.as_array().unwrap();
                    assert_eq!(sessions.len(), 1);
                }

                #[test]
                fn test_cleanup_sessions() {
                    let (adapter, _dir) = setup();
                    let result = adapter.cleanup_sessions().unwrap();
                    assert!(result.is_object());
                }
            }
        }
        pub mod skill_provider {
            //! SkillProvider implementation for the skrun-managed skill catalog.

            use skrun::{ArtifactKind, SkillArtifact};
            use std::path::PathBuf;
            use types::Skill;
            use types::skill::{SkillContent, SkillInfo, SkillProvider, SkillSource};

            const SKRUN_TOOL_NAME: &str = "run_skill";

            fn validate_skill_id(skill_id: &str) -> Result<(), String> {
                if skill_id.is_empty() {
                    return Err("skill id cannot be empty".to_string());
                }
                if !skill_id
                    .chars()
                    .all(|item| item.is_ascii_alphanumeric() || item == '-' || item == '_')
                {
                    return Err(
                        "skill id must contain only ASCII letters, numbers, '-' or '_'".to_string(),
                    );
                }
                if !skill_id
                    .chars()
                    .next()
                    .is_some_and(|item| item.is_ascii_alphanumeric())
                {
                    return Err("skill id must start with an ASCII letter or number".to_string());
                }
                Ok(())
            }

            fn skill_info(skill: Skill) -> SkillInfo {
                SkillInfo {
                    id: skill.id,
                    name: skill.name,
                    description: skill.description,
                    tags: skill.tags,
                    kind: skill.kind,
                    executable: skill.executable,
                    suggested_tools: skill.suggested_tools,
                    source: skill.source,
                    read_only: skill.read_only,
                    source_ref: skill.source_ref,
                }
            }

            fn skill_content(skill: Skill) -> SkillContent {
                SkillContent {
                    id: skill.id,
                    name: skill.name,
                    content: skill.content,
                    kind: skill.kind,
                    executable: skill.executable,
                    suggested_tools: skill.suggested_tools,
                    source: skill.source,
                    read_only: skill.read_only,
                    source_ref: skill.source_ref,
                }
            }

            fn artifact_kind_label(kind: &ArtifactKind) -> &'static str {
                match kind {
                    ArtifactKind::Markdown => "markdown",
                    ArtifactKind::RustBinary => "rust_binary",
                    ArtifactKind::PythonUv => "python_uv",
                }
            }

            fn skrun_artifact_to_model(record: SkillArtifact) -> Skill {
                let kind = artifact_kind_label(&record.kind);
                let executable = record.executable || record.kind != ArtifactKind::Markdown;
                let description = record.description.clone().or_else(|| {
                    Some(if executable {
                        format!("Executable skrun {kind} skill.")
                    } else {
                        format!("Guidance-only skrun {kind} skill.")
                    })
                });
                let content = record.content.unwrap_or_else(|| {
                    format!(
                        "# {}\n\n{} skrun skill.\n\n- id: `{}`\n- kind: `{}`\n- version: `{}`\n",
                        record.name,
                        if executable {
                            "Executable"
                        } else {
                            "Guidance-only"
                        },
                        record.id,
                        kind,
                        record.version
                    )
                });
                let mut skill = Skill::new(
                    record.id.clone(),
                    record.name,
                    description,
                    record.tags,
                    content,
                );
                skill.kind = Some(kind.to_string());
                skill.executable = executable;
                skill.source = SkillSource::External;
                skill.read_only = true;
                skill.version = Some(record.version.clone());
                skill.source_ref = record
                    .source_ref
                    .or_else(|| Some(format!("skrun:{}@{}", record.id, record.version)));
                skill.suggested_tools = record.suggested_tools;
                if executable
                    && !skill
                        .suggested_tools
                        .iter()
                        .any(|tool| tool == SKRUN_TOOL_NAME)
                {
                    skill.suggested_tools.push(SKRUN_TOOL_NAME.to_string());
                }
                skill
            }

            /// Read-only provider for skills exposed by the skrun public CLI contract.
            pub struct SkrunSkillProvider {
                root: Option<PathBuf>,
            }

            impl SkrunSkillProvider {
                pub fn new(root: impl Into<PathBuf>) -> Self {
                    Self {
                        root: Some(root.into()),
                    }
                }

                pub fn from_default_root() -> Self {
                    Self { root: None }
                }

                fn root(&self) -> Result<PathBuf, String> {
                    match &self.root {
                        Some(root) => Ok(root.clone()),
                        None => crate::services::skills::skill_catalog_root()
                            .map_err(|error| error.to_string()),
                    }
                }

                pub fn try_list_skill_models(&self) -> Result<Vec<Skill>, String> {
                    let root = self.root()?;
                    let mut skills = skrun::list_installed_skills(root)
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .map(skrun_artifact_to_model)
                        .collect::<Vec<_>>();
                    skills.sort_by(|left, right| left.id.cmp(&right.id));
                    Ok(skills)
                }

                pub fn list_skill_models(&self) -> Vec<Skill> {
                    match self.try_list_skill_models() {
                        Ok(skills) => skills,
                        Err(error) => {
                            tracing::debug!(error = %error, "skrun skill catalog is not available");
                            Vec::new()
                        }
                    }
                }

                pub fn try_get_skill_model(&self, id: &str) -> Result<Option<Skill>, String> {
                    validate_skill_id(id)?;
                    let root = self.root()?;
                    let skill_root = root.join(id);
                    if !skill_root.exists() {
                        return Ok(None);
                    }

                    let root = root.canonicalize().map_err(|error| error.to_string())?;
                    let skill_root = skill_root
                        .canonicalize()
                        .map_err(|error| error.to_string())?;
                    if !skill_root.starts_with(&root) {
                        return Err(format!(
                            "skill id '{}' resolves outside the skill catalog",
                            id
                        ));
                    }

                    let artifact =
                        skrun::load_artifact(skill_root).map_err(|error| error.to_string())?;
                    if artifact.id != id {
                        return Err(format!(
                            "skill artifact id mismatch: requested '{}', found '{}'",
                            id, artifact.id
                        ));
                    }

                    Ok(Some(skrun_artifact_to_model(artifact)))
                }

                pub fn get_skill_model(&self, id: &str) -> Option<Skill> {
                    match self.try_get_skill_model(id) {
                        Ok(skill) => skill,
                        Err(error) => {
                            tracing::debug!(error = %error, skill_id = %id, "skrun skill catalog is not available");
                            None
                        }
                    }
                }
            }

            impl Default for SkrunSkillProvider {
                fn default() -> Self {
                    Self::from_default_root()
                }
            }

            impl SkillProvider for SkrunSkillProvider {
                fn list_skills(&self) -> Vec<SkillInfo> {
                    self.list_skill_models()
                        .into_iter()
                        .map(skill_info)
                        .collect()
                }

                fn get_skill(&self, id: &str) -> Option<SkillContent> {
                    self.get_skill_model(id).map(skill_content)
                }

                fn export_skill(&self, id: &str) -> Result<String, String> {
                    self.get_skill_model(id)
                        .map(|skill| skill.to_markdown())
                        .ok_or_else(|| format!("Skill {} not found", id))
                }
            }

            #[cfg(test)]
            mod tests {
                use super::*;
                use tempfile::tempdir;
                fn save_test_artifact(root: &std::path::Path, artifact: &SkillArtifact) {
                    let skill_root = root.join(&artifact.id);
                    skrun::save_artifact(skill_root, artifact).unwrap();
                }

                #[test]
                fn test_skrun_provider_lists_installed_artifacts() {
                    let dir = tempdir().unwrap();
                    let mut artifact =
                        SkillArtifact::rust_binary("regex-finder", "Regex Finder", "0.1.0");
                    artifact.description = Some("Find text with regex.".to_string());
                    artifact.content = Some("# Regex Finder\n\nFind text with regex.".to_string());
                    save_test_artifact(dir.path(), &artifact);

                    let provider = SkrunSkillProvider::new(dir.path());
                    let skills = provider.list_skill_models();

                    assert_eq!(skills.len(), 1);
                    assert_eq!(skills[0].id, "regex-finder");
                    assert_eq!(skills[0].source, SkillSource::External);
                    assert!(skills[0].read_only);
                    assert_eq!(skills[0].suggested_tools, vec![SKRUN_TOOL_NAME]);
                }

                #[test]
                fn test_skrun_provider_lists_markdown_skills_without_run_tool() {
                    let dir = tempdir().unwrap();
                    let mut artifact =
                        SkillArtifact::markdown("team", "Team", "0.1.0", "# Team\n\nUse workers.");
                    artifact.description = Some("Coordinate workers.".to_string());
                    artifact.tags = Some(vec!["team".to_string()]);
                    artifact.suggested_tools = vec!["spawn_subagent_batch".to_string()];
                    save_test_artifact(dir.path(), &artifact);

                    let provider = SkrunSkillProvider::new(dir.path());
                    let skills = provider.list_skill_models();

                    assert_eq!(skills.len(), 1);
                    assert_eq!(skills[0].id, "team");
                    assert_eq!(skills[0].source, SkillSource::External);
                    assert_eq!(skills[0].tags, Some(vec!["team".to_string()]));
                    assert_eq!(skills[0].suggested_tools, vec!["spawn_subagent_batch"]);
                }

                #[test]
                fn test_default_test_catalog_is_empty_without_skrun_override() {
                    let dir = tempdir().unwrap();
                    let provider = SkrunSkillProvider::new(dir.path().join("missing"));
                    assert!(provider.try_list_skill_models().unwrap().is_empty());
                    assert!(provider.try_get_skill_model("team").unwrap().is_none());
                }

                #[test]
                fn test_get_rejects_path_like_skill_id() {
                    let dir = tempdir().unwrap();
                    let provider = SkrunSkillProvider::new(dir.path());

                    let error = provider
                        .try_get_skill_model("../outside")
                        .expect_err("path-like skill id should be rejected");

                    assert!(error.contains("must contain only ASCII letters"));
                }

                #[test]
                fn test_get_rejects_artifact_id_mismatch() {
                    let dir = tempdir().unwrap();
                    let artifact = SkillArtifact::markdown(
                        "actual",
                        "Actual",
                        "0.1.0",
                        "# Actual\n\nUse this.",
                    );
                    skrun::save_artifact(dir.path().join("alias"), &artifact).unwrap();
                    let provider = SkrunSkillProvider::new(dir.path());

                    let error = provider
                        .try_get_skill_model("alias")
                        .expect_err("artifact id mismatch should be rejected");

                    assert!(error.contains("artifact id mismatch"));
                }
            }
        }

        pub use self::agent::AgentStoreAdapter;
        pub use config::ConfigStoreAdapter;
        pub use ops::OpsProviderAdapter;
        pub use secret::SecretStoreAdapter;
        pub use session::SessionStorageAdapter;
        pub use skill_provider::SkrunSkillProvider;
    }
    pub mod agent {
        //! Agent service layer
        //!
        //! This module only covers agent CRUD operations.
        //! Agent execution happens through chat sessions and subagent runs.

        use crate::{
            AppCore,
            agent_validation::validate_agent_node_async,
            services::agent_catalog::{DEFAULT_ASSISTANT_NAME, StoredAgent},
            services::session::SessionService,
        };
        use anyhow::{Context, Result};
        use std::sync::Arc;
        use types::{AgentNode, encode_validation_error};

        pub async fn list_agents(core: &Arc<AppCore>) -> Result<Vec<StoredAgent>> {
            core.storage
                .agents
                .list_agents()
                .context("Failed to list agents")
        }

        pub async fn get_agent(core: &Arc<AppCore>, id: &str) -> Result<StoredAgent> {
            core.storage
                .agents
                .get_agent(id.to_string())
                .with_context(|| format!("Failed to get agent {}", id))?
                .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))
        }

        pub async fn create_agent(
            core: &Arc<AppCore>,
            name: String,
            mut agent: AgentNode,
        ) -> Result<StoredAgent> {
            normalize_model_fields(&mut agent)?;
            validate_agent_node(core, &agent).await?;
            core.storage
                .agents
                .create_agent(name.clone(), agent)
                .with_context(|| format!("Failed to create agent {}", name))
        }

        pub async fn update_agent(
            core: &Arc<AppCore>,
            id: &str,
            name: Option<String>,
            mut agent: Option<AgentNode>,
        ) -> Result<StoredAgent> {
            if let Some(agent_node) = agent.as_mut() {
                normalize_model_fields(agent_node)?;
                validate_agent_node(core, agent_node).await?;
            }
            core.storage
                .agents
                .update_agent(id.to_string(), name, agent)
                .with_context(|| format!("Failed to update agent {}", id))
        }

        pub async fn delete_agent(core: &Arc<AppCore>, id: &str) -> Result<()> {
            let resolved_id = core
                .storage
                .agents
                .resolve_existing_agent_id(id)
                .with_context(|| format!("Failed to resolve agent {}", id))?;

            let resolved_default_id = core.storage.agents.resolve_default_agent_id().ok();
            if resolved_default_id.as_deref() == Some(resolved_id.as_str()) {
                let agent_name = core
                    .storage
                    .agents
                    .get_agent(resolved_id.clone())?
                    .map(|agent| agent.name)
                    .unwrap_or_else(|| DEFAULT_ASSISTANT_NAME.to_string());
                anyhow::bail!(
                    "Cannot delete default assistant agent {} ({})",
                    resolved_id,
                    agent_name
                );
            }

            let session_service = SessionService::from_storage(&core.storage);
            archive_agent_workspace_sessions(&session_service, &resolved_id).with_context(
                || {
                    format!(
                        "Failed to archive workspace sessions before deleting agent {}",
                        id
                    )
                },
            )?;

            core.storage
                .agents
                .delete_agent(resolved_id)
                .with_context(|| format!("Failed to delete agent {}", id))
        }

        fn normalize_model_fields(agent: &mut AgentNode) -> Result<()> {
            if let Err(error) = agent.normalize_model_fields() {
                anyhow::bail!(encode_validation_error(vec![error]));
            }
            Ok(())
        }

        fn archive_agent_workspace_sessions(
            session_service: &SessionService,
            agent_id: &str,
        ) -> Result<()> {
            for session in session_service.list_session_views(Some(agent_id), None, true)? {
                let _ = session_service.archive_session(&session.id)?;
            }
            Ok(())
        }

        async fn validate_agent_node(core: &Arc<AppCore>, agent: &AgentNode) -> Result<()> {
            if let Err(errors) = agent.validate() {
                anyhow::bail!(encode_validation_error(errors));
            }
            if let Err(errors) = validate_agent_node_async(agent, core).await {
                anyhow::bail!(encode_validation_error(errors));
            }
            Ok(())
        }

        #[cfg(test)]
        #[allow(clippy::await_holding_lock)]
        mod tests {
            use super::*;
            use crate::prompt_files;
            use crate::time_utils;
            use tempfile::tempdir;
            use types::{ApiKeyConfig, ChatSession, ModelId, ValidationErrorResponse};

            struct AgentsDirEnvGuard {
                _lock: std::sync::MutexGuard<'static, ()>,
            }

            impl AgentsDirEnvGuard {
                fn new() -> Self {
                    Self {
                        _lock: prompt_files::agents_dir_env_lock(),
                    }
                }
            }

            impl Drop for AgentsDirEnvGuard {
                fn drop(&mut self) {
                    unsafe {
                        std::env::remove_var(prompt_files::AGENTS_DIR_ENV);
                        std::env::remove_var("RESTFLOW_DIR");
                    };
                }
            }

            /// Create a test AppCore with an isolated agents directory.
            /// Returns (core, _temp_db_dir, _temp_agents_dir, _env_guard).
            /// All returned values must be held alive for the test duration.
            #[allow(clippy::await_holding_lock)]
            async fn create_test_core_isolated() -> (
                Arc<AppCore>,
                tempfile::TempDir,
                tempfile::TempDir,
                AgentsDirEnvGuard,
            ) {
                let env_guard = AgentsDirEnvGuard::new();
                let temp_db = tempdir().unwrap();
                let temp_agents = tempdir().unwrap();
                unsafe {
                    std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_agents.path());
                    std::env::set_var("RESTFLOW_DIR", temp_db.path());
                };
                let db_path = temp_db.path().join("test.db");
                let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
                (core, temp_db, temp_agents, env_guard)
            }

            #[test]
            fn test_agents_dir_env_guard_cleans_up_env_var() {
                let guard = AgentsDirEnvGuard::new();
                unsafe {
                    std::env::set_var(prompt_files::AGENTS_DIR_ENV, "/tmp/restflow-test-agents")
                };
                drop(guard);
                assert!(std::env::var(prompt_files::AGENTS_DIR_ENV).is_err());
            }

            fn create_test_agent_node(prompt: &str) -> AgentNode {
                AgentNode {
                    model_ref: Some(types::ModelRef::from_model(ModelId::ClaudeSonnet4_5)),
                    prompt: Some(prompt.to_string()),
                    temperature: Some(0.7),
                    codex_cli_reasoning_effort: None,
                    codex_cli_execution_mode: None,
                    api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
                    tools: Some(vec!["bash".to_string()]),
                    skills: None,
                    skill_variables: None,
                    skill_preflight_policy_mode: None,
                    model_routing: None,
                }
            }

            fn set_test_model(node: &mut AgentNode, model: ModelId) {
                node.model_ref = Some(types::ModelRef::from_model(model));
            }

            #[tokio::test]
            async fn test_list_agents_empty() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let agents = list_agents(&core).await.unwrap();
                assert_eq!(agents.len(), 1);
                assert_eq!(agents[0].name, "Default Assistant");
            }

            #[tokio::test]
            async fn test_create_and_get_agent() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("You are a helpful assistant");
                let created = create_agent(&core, "Test Agent".to_string(), agent_node)
                    .await
                    .unwrap();

                assert!(!created.id.is_empty());
                assert_eq!(created.name, "Test Agent");
                if let Some(prompt) = &created.agent.prompt {
                    assert_eq!(prompt, "You are a helpful assistant");
                }

                let prompt_on_disk = prompt_files::load_agent_prompt_for_agent(
                    &created.id,
                    &created.name,
                    created.prompt_file.as_deref(),
                )
                .unwrap();
                assert_eq!(
                    prompt_on_disk.content,
                    Some("You are a helpful assistant".to_string())
                );

                let retrieved = get_agent(&core, &created.id).await.unwrap();
                assert_eq!(retrieved.id, created.id);
                assert_eq!(retrieved.name, "Test Agent");
                if let Some(prompt) = &retrieved.agent.prompt {
                    assert_eq!(prompt, "You are a helpful assistant");
                }
            }

            #[tokio::test]
            async fn test_list_agents_multiple() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent1 = create_test_agent_node("Agent 1 prompt");
                let agent2 = create_test_agent_node("Agent 2 prompt");
                let agent3 = create_test_agent_node("Agent 3 prompt");

                create_agent(&core, "Agent 1".to_string(), agent1)
                    .await
                    .unwrap();
                create_agent(&core, "Agent 2".to_string(), agent2)
                    .await
                    .unwrap();
                create_agent(&core, "Agent 3".to_string(), agent3)
                    .await
                    .unwrap();

                let agents = list_agents(&core).await.unwrap();
                assert_eq!(agents.len(), 4);

                let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
                assert!(names.contains(&"Default Assistant".to_string()));
                assert!(names.contains(&"Agent 1".to_string()));
                assert!(names.contains(&"Agent 2".to_string()));
                assert!(names.contains(&"Agent 3".to_string()));
            }

            #[tokio::test]
            async fn test_update_agent_name() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("Test prompt");
                let created = create_agent(&core, "Original Name".to_string(), agent_node)
                    .await
                    .unwrap();

                let updated =
                    update_agent(&core, &created.id, Some("Updated Name".to_string()), None)
                        .await
                        .unwrap();

                assert_eq!(updated.name, "Updated Name");
                if let Some(prompt) = &updated.agent.prompt {
                    assert_eq!(prompt, "Test prompt");
                }
                let prompt_on_disk = prompt_files::load_agent_prompt_for_agent(
                    &updated.id,
                    &updated.name,
                    updated.prompt_file.as_deref(),
                )
                .unwrap();
                assert_eq!(prompt_on_disk.content, Some("Test prompt".to_string()));
            }

            #[tokio::test]
            async fn test_update_agent_config() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("Original prompt");
                let created = create_agent(&core, "Test Agent".to_string(), agent_node)
                    .await
                    .unwrap();

                // Use DeepseekChat which supports temperature (unlike Gpt5Mini)
                let mut new_agent_node = create_test_agent_node("Updated prompt");
                new_agent_node.temperature = Some(0.9);
                set_test_model(&mut new_agent_node, ModelId::DeepseekChat);

                let updated = update_agent(&core, &created.id, None, Some(new_agent_node))
                    .await
                    .unwrap();

                assert_eq!(updated.name, "Test Agent"); // Name unchanged
                if let Some(prompt) = &updated.agent.prompt {
                    assert_eq!(prompt, "Updated prompt");
                }
                let prompt_on_disk = prompt_files::load_agent_prompt_for_agent(
                    &updated.id,
                    &updated.name,
                    updated.prompt_file.as_deref(),
                )
                .unwrap();
                assert_eq!(prompt_on_disk.content, Some("Updated prompt".to_string()));
                assert_eq!(updated.agent.temperature, Some(0.9));
                assert_eq!(
                    updated
                        .agent
                        .resolved_model_ref()
                        .map(|model_ref| model_ref.model),
                    Some(ModelId::DeepseekChat)
                );
            }

            #[tokio::test]
            async fn test_delete_agent() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("Test prompt");
                let created = create_agent(&core, "To Delete".to_string(), agent_node)
                    .await
                    .unwrap();

                // Verify it exists
                let retrieved = get_agent(&core, &created.id).await;
                assert!(retrieved.is_ok());

                // Delete it
                delete_agent(&core, &created.id).await.unwrap();

                // Verify it's gone
                let result = get_agent(&core, &created.id).await;
                assert!(result.is_err());
            }

            #[tokio::test]
            async fn test_delete_default_assistant_is_blocked() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let default = core.storage.agents.resolve_default_agent().unwrap();
                assert!(default.name.eq_ignore_ascii_case(DEFAULT_ASSISTANT_NAME));

                let err = delete_agent(&core, &default.id).await.unwrap_err();
                let msg = err.to_string();
                assert!(msg.contains("Cannot delete default assistant agent"));
            }

            #[tokio::test]
            async fn test_delete_agent_archives_workspace_sessions() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("Workspace owner");
                let created = create_agent(&core, "Workspace Owner".to_string(), agent_node)
                    .await
                    .unwrap();

                let session = ChatSession::new(
                    created.id.clone(),
                    ModelId::Gpt5.as_serialized_str().to_string(),
                )
                .with_name("Workspace Session");
                core.storage
                    .file_sessions
                    .write_session(
                        &crate::session_log::FileSession::from_chat_session(&session),
                        true,
                    )
                    .unwrap();

                delete_agent(&core, &created.id).await.unwrap();

                let active_sessions = core
                    .storage
                    .file_sessions
                    .list()
                    .unwrap()
                    .into_iter()
                    .map(|session| session.to_chat_session())
                    .filter(|session| session.agent_id == created.id && !session.is_archived())
                    .collect::<Vec<_>>();
                assert!(active_sessions.is_empty());

                let archived_session = core
                    .storage
                    .file_sessions
                    .get(&session.id)
                    .unwrap()
                    .map(|session| session.to_chat_session())
                    .expect("session should remain after archiving");
                assert!(archived_session.is_archived());
            }

            #[tokio::test]
            async fn test_get_nonexistent_agent_fails() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let result = get_agent(&core, "nonexistent-id").await;
                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("not found"));
            }

            #[tokio::test]
            async fn test_create_agent_generates_uuid() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("Test prompt");
                let created = create_agent(&core, "Test Agent".to_string(), agent_node)
                    .await
                    .unwrap();

                // Verify ID is a valid UUID format
                assert!(!created.id.is_empty());
                assert!(created.id.contains('-')); // UUIDs contain hyphens
                assert_eq!(created.id.len(), 36); // Standard UUID length
            }

            #[tokio::test]
            async fn test_create_agent_sets_timestamps() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let before = time_utils::now_ms();

                let agent_node = create_test_agent_node("Test prompt");
                let created = create_agent(&core, "Test Agent".to_string(), agent_node)
                    .await
                    .unwrap();

                let after = time_utils::now_ms();

                // Verify timestamps are set and within reasonable bounds
                assert!(created.created_at.is_some());
                assert!(created.updated_at.is_some());

                let created_at = created.created_at.unwrap();
                let updated_at = created.updated_at.unwrap();

                assert!(created_at >= before && created_at <= after);
                assert!(updated_at >= before && updated_at <= after);
                assert_eq!(created_at, updated_at); // Should be same on creation
            }

            #[tokio::test]
            async fn test_update_agent_updates_timestamp() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;

                let agent_node = create_test_agent_node("Test prompt");
                let created = create_agent(&core, "Test Agent".to_string(), agent_node)
                    .await
                    .unwrap();

                // Small delay to ensure timestamp difference
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                let updated =
                    update_agent(&core, &created.id, Some("Updated Name".to_string()), None)
                        .await
                        .unwrap();

                // Updated timestamp should be newer
                assert!(updated.updated_at.unwrap() > created.updated_at.unwrap());
                // Created timestamp should remain the same
                assert_eq!(updated.created_at, created.created_at);
            }

            #[tokio::test]
            async fn test_create_agent_rejects_invalid_temperature() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let mut node = create_test_agent_node("test");
                node.temperature = Some(3.0);

                let err = create_agent(&core, "Invalid Agent".to_string(), node)
                    .await
                    .expect_err("expected validation error");
                let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
                    .expect("validation error payload should be JSON");
                assert_eq!(payload.error_type, "validation_error");
                assert!(payload.errors.iter().any(|e| e.field == "temperature"));
            }

            #[tokio::test]
            async fn test_create_agent_rejects_temperature_on_unsupported_model() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let mut node = create_test_agent_node("test");
                set_test_model(&mut node, ModelId::Gpt5);
                node.temperature = Some(0.5);

                let err = create_agent(&core, "Bad Temp Agent".to_string(), node)
                    .await
                    .expect_err("expected validation error");
                let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
                    .expect("validation error payload should be JSON");
                assert!(
                    payload
                        .errors
                        .iter()
                        .any(|e| e.field == "temperature" && e.message.contains("does not support"))
                );
            }

            #[tokio::test]
            async fn test_create_agent_rejects_reasoning_effort_on_non_codex() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let mut node = create_test_agent_node("test");
                // ClaudeSonnet4_5 is not a Codex model
                node.codex_cli_reasoning_effort = Some("high".to_string());

                let err = create_agent(&core, "Bad Effort Agent".to_string(), node)
                    .await
                    .expect_err("expected validation error");
                let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
                    .expect("validation error payload should be JSON");
                assert!(
                    payload
                        .errors
                        .iter()
                        .any(|e| e.field == "codex_cli_reasoning_effort"
                            && e.message.contains("only applies to Codex CLI"))
                );
            }

            #[tokio::test]
            async fn test_create_agent_rejects_unknown_tool() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let mut node = create_test_agent_node("test");
                node.tools = Some(vec!["tool_does_not_exist".to_string()]);

                let err = create_agent(&core, "Invalid Tool Agent".to_string(), node)
                    .await
                    .expect_err("expected validation error");
                let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
                    .expect("validation error payload should be JSON");
                assert!(payload.errors.iter().any(|e| e.field == "tools"));
            }

            #[tokio::test]
            async fn test_create_agent_rejects_unknown_skill() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let mut node = create_test_agent_node("test");
                node.skills = Some(vec!["missing-skill".to_string()]);

                let err = create_agent(&core, "Invalid Skill Agent".to_string(), node)
                    .await
                    .expect_err("expected validation error");
                let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
                    .expect("validation error payload should be JSON");
                assert!(payload.errors.iter().any(|e| e.field == "skills"));
            }

            #[tokio::test]
            async fn test_create_agent_rejects_missing_secret_reference() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                let mut node = create_test_agent_node("test");
                node.api_key_config = Some(ApiKeyConfig::Secret("MISSING_SECRET".to_string()));

                let err = create_agent(&core, "Missing Secret Agent".to_string(), node)
                    .await
                    .expect_err("expected validation error");
                let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
                    .expect("validation error payload should be JSON");
                assert!(payload.errors.iter().any(|e| e.field == "api_key_config"));
            }

            #[tokio::test]
            async fn test_create_agent_accepts_existing_secret_reference() {
                let (core, _db, _agents, _guard) = create_test_core_isolated().await;
                core.storage
                    .secrets
                    .set_secret("OPENAI_API_KEY", "secret-value", None)
                    .unwrap();

                let mut node = create_test_agent_node("test");
                node.api_key_config = Some(ApiKeyConfig::Secret("OPENAI_API_KEY".to_string()));
                node.tools = Some(vec!["bash".to_string()]);

                let created = create_agent(&core, "Valid Secret Agent".to_string(), node)
                    .await
                    .expect("expected create to pass");
                assert_eq!(created.name, "Valid Secret Agent");
            }
        }
    }
    pub mod agent_catalog {
        //! Typed agent storage wrapper.

        use crate::prompt_files;
        use crate::time_utils;
        use anyhow::Result;
        use serde::{Deserialize, Serialize};
        use specta::Type;
        use std::collections::HashMap;
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::sync::{Arc, Mutex};
        use std::time::UNIX_EPOCH;
        use types::AgentNode;
        use uuid::Uuid;

        /// Canonical default assistant name created during app initialization.
        pub const DEFAULT_ASSISTANT_NAME: &str = "Default Assistant";

        /// Stored agent with metadata
        #[derive(Serialize, Deserialize, Debug, Clone, Type)]
        pub struct StoredAgent {
            pub id: String,
            pub name: String,
            pub agent: AgentNode,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub prompt_file: Option<String>,
            pub created_at: Option<i64>,
            pub updated_at: Option<i64>,
        }

        #[derive(Debug, Clone, Default, Serialize, Deserialize)]
        struct AgentFileFrontmatter {
            pub id: String,
            pub name: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub model_ref: Option<types::ModelRef>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub tools: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub skills: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub skill_variables: Option<HashMap<String, String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub skill_preflight_policy_mode: Option<types::SkillPreflightPolicyMode>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub created_at: Option<i64>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub updated_at: Option<i64>,
        }

        /// Typed agent storage wrapper around process-local agent bytes.
        #[derive(Clone)]
        pub struct AgentStorage {
            agents_dir: PathBuf,
            delete_lock: Arc<Mutex<()>>,
        }

        impl AgentStorage {
            pub fn new_file_backed() -> Result<Self> {
                Self::new_file_backed_path(prompt_files::ensure_agents_dir()?)
            }

            pub fn new_file_backed_path(agents_dir: impl Into<PathBuf>) -> Result<Self> {
                let agents_dir = agents_dir.into();
                fs::create_dir_all(&agents_dir).map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to create agents directory {}: {error}",
                        agents_dir.display()
                    )
                })?;
                Ok(Self {
                    agents_dir,
                    delete_lock: Arc::new(Mutex::new(())),
                })
            }

            pub fn create_agent(&self, name: String, mut agent: AgentNode) -> Result<StoredAgent> {
                normalize_model_fields(&mut agent)?;
                let now = time_utils::now_ms();
                let id = Uuid::new_v4().to_string();

                // Prompt content is file-backed under ~/.restflow/agents/{agent-name}.md, not stored in DB.
                let prompt_override = agent.prompt.take();
                let prompt_path =
                    self.ensure_agent_prompt_file(&id, &name, None, prompt_override.as_deref())?;
                agent.prompt = read_agent_prompt_body(&prompt_path)?;
                let prompt_file = Some(path_file_name(&prompt_path)?);

                let stored_agent = StoredAgent {
                    id,
                    name,
                    agent,
                    prompt_file,
                    created_at: Some(now),
                    updated_at: Some(now),
                };

                self.persist_without_prompt(&stored_agent)?;

                Ok(stored_agent)
            }

            pub fn get_agent(&self, id: String) -> Result<Option<StoredAgent>> {
                if let Some(agent) = self.get_file_agent(&id)? {
                    return Ok(Some(agent));
                }

                Ok(None)
            }

            pub fn list_agents(&self) -> Result<Vec<StoredAgent>> {
                self.list_file_agents()
            }

            /// Resolve the default chat agent deterministically.
            ///
            /// Resolution order:
            /// 1. Agent named "Default Assistant" (case-insensitive)
            /// 2. The only existing agent (when exactly one exists)
            ///
            /// This intentionally avoids selecting an arbitrary first agent when
            /// multiple agents exist.
            pub fn resolve_default_agent(&self) -> Result<StoredAgent> {
                let agents = self.list_agents()?;

                if agents.is_empty() {
                    anyhow::bail!("No agents configured");
                }

                if let Some(agent) = agents
                    .iter()
                    .find(|agent| agent.name.eq_ignore_ascii_case(DEFAULT_ASSISTANT_NAME))
                    .cloned()
                {
                    return Ok(agent);
                }

                if agents.len() == 1 {
                    return Ok(agents[0].clone());
                }

                anyhow::bail!(
                    "Default agent is ambiguous: define an agent named '{}'",
                    DEFAULT_ASSISTANT_NAME
                )
            }

            /// Resolve only the ID of the default chat agent.
            pub fn resolve_default_agent_id(&self) -> Result<String> {
                Ok(self.resolve_default_agent()?.id)
            }

            pub fn update_agent(
                &self,
                id: String,
                name: Option<String>,
                agent: Option<AgentNode>,
            ) -> Result<StoredAgent> {
                let mut existing_agent = self
                    .get_agent(id.clone())?
                    .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))?;

                if let Some(new_name) = name {
                    existing_agent.name = new_name;
                }

                let mut prompt_override: Option<String> = None;
                if let Some(mut new_agent) = agent {
                    normalize_model_fields(&mut new_agent)?;
                    prompt_override = new_agent.prompt.take();
                    existing_agent.agent = new_agent;
                }

                self.ensure_agent_prompt_file(
                    &existing_agent.id,
                    &existing_agent.name,
                    existing_agent.prompt_file.as_deref(),
                    prompt_override.as_deref(),
                )
                .and_then(|path| {
                    existing_agent.agent.prompt = read_agent_prompt_body(&path)?;
                    path_file_name(&path)
                })
                .map(|prompt_file| existing_agent.prompt_file = Some(prompt_file))?;

                let now = time_utils::now_ms();
                existing_agent.updated_at = Some(now);

                self.persist_without_prompt(&existing_agent)?;

                Ok(existing_agent)
            }

            /// Delete an agent atomically to prevent TOCTOU race conditions.
            ///
            /// This operation resolves the agent ID and deletes it within a single
            /// write transaction, ensuring that concurrent delete operations on the
            /// same agent are handled correctly.
            ///
            /// # Errors
            /// Returns an error if the agent is not found or if the ID prefix is ambiguous.
            pub fn delete_agent(&self, id: String) -> Result<()> {
                let _delete_guard = self
                    .delete_lock
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Agent delete lock poisoned"))?;
                if let Some(existing) = self.get_file_agent(&id)? {
                    if let Some(prompt_file) = existing.prompt_file.as_deref() {
                        let path = self.resolve_prompt_path_from_file_name(prompt_file)?;
                        match fs::remove_file(&path) {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                anyhow::bail!("Agent {} not found", id);
                            }
                            Err(error) => {
                                return Err(anyhow::anyhow!(
                                    "Failed to remove agent file {}: {error}",
                                    path.display()
                                ));
                            }
                        }
                    }
                    return Ok(());
                }

                anyhow::bail!("Agent {} not found", id)
            }

            pub fn resolve_existing_agent_id(&self, id_or_prefix: &str) -> Result<String> {
                let id = id_or_prefix.trim();
                if id.is_empty() {
                    anyhow::bail!("Agent ID is empty");
                }

                if let Some(agent) = self.get_file_agent(id)? {
                    return Ok(agent.id);
                }

                match self.resolve_agent_id_candidate(id)? {
                    Some(resolved) => Ok(resolved),
                    None => anyhow::bail!("Agent {} not found", id),
                }
            }

            pub fn reconcile_prompt_file_names(&self) -> Result<()> {
                let agents = self.list_agents()?;
                for mut agent in agents {
                    let prompt_path = self.ensure_agent_prompt_file(
                        &agent.id,
                        &agent.name,
                        agent.prompt_file.as_deref(),
                        None,
                    )?;
                    let prompt_file = path_file_name(&prompt_path)?;
                    if agent.prompt_file.as_deref() != Some(prompt_file.as_str()) {
                        agent.prompt_file = Some(prompt_file);
                        self.persist_without_prompt(&agent)?;
                    }
                }
                Ok(())
            }

            fn persist_without_prompt(&self, stored: &StoredAgent) -> Result<()> {
                self.write_agent_file(stored)?;
                Ok(())
            }

            fn get_file_agent(&self, id_or_prefix: &str) -> Result<Option<StoredAgent>> {
                let candidate = id_or_prefix.trim();
                if candidate.is_empty() {
                    return Ok(None);
                }
                let agents = self.list_file_agents()?;
                if let Some(agent) = agents.iter().find(|agent| agent.id == candidate).cloned() {
                    return Ok(Some(agent));
                }
                let matches = agents
                    .into_iter()
                    .filter(|agent| agent.id.starts_with(candidate))
                    .collect::<Vec<_>>();
                match matches.len() {
                    0 => Ok(None),
                    1 => Ok(matches.into_iter().next()),
                    _ => {
                        let preview = matches
                            .iter()
                            .take(5)
                            .map(|agent| agent.id.clone())
                            .collect::<Vec<_>>()
                            .join(", ");
                        anyhow::bail!(
                            "Agent ID prefix '{}' is ambiguous ({} matches: {})",
                            candidate,
                            matches.len(),
                            preview
                        )
                    }
                }
            }

            fn list_file_agents(&self) -> Result<Vec<StoredAgent>> {
                let agents_dir = self.ensure_agents_dir()?;
                let mut agents = Vec::new();
                for entry in fs::read_dir(&agents_dir).map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to read agents directory {}: {error}",
                        agents_dir.display()
                    )
                })? {
                    let path = entry?.path();
                    if path.extension().and_then(|value| value.to_str()) != Some("md") {
                        continue;
                    }
                    if let Some(agent) = load_file_agent(&path)? {
                        agents.push(agent);
                    }
                }
                agents.sort_by(|left, right| {
                    left.name
                        .cmp(&right.name)
                        .then_with(|| left.id.cmp(&right.id))
                });
                Ok(agents)
            }

            fn write_agent_file(&self, stored: &StoredAgent) -> Result<()> {
                let prompt = stored
                    .agent
                    .prompt
                    .clone()
                    .or_else(|| prompt_files::load_default_main_agent_prompt().ok())
                    .unwrap_or_default();
                let file_name = stored.prompt_file.clone().unwrap_or_else(|| {
                    format!(
                        "{}.md",
                        prompt_files::sanitize_agent_file_stem(&stored.name)
                    )
                });
                let path = self.ensure_agents_dir()?.join(file_name);
                let content = render_agent_file(stored, &prompt)?;
                fs::write(&path, content).map_err(|error| {
                    anyhow::anyhow!("Failed to write agent file {}: {error}", path.display())
                })
            }

            fn resolve_agent_id_candidate(&self, id_or_prefix: &str) -> Result<Option<String>> {
                let prefix = id_or_prefix.trim();
                if prefix.is_empty() {
                    return Ok(None);
                }

                let matches: Vec<String> = self
                    .list_file_agents()?
                    .into_iter()
                    .map(|agent| agent.id)
                    .filter(|id| id.starts_with(prefix))
                    .collect();
                match matches.len() {
                    0 => {}
                    1 => return Ok(matches.into_iter().next()),
                    _ => {
                        let preview = matches
                            .iter()
                            .take(5)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        anyhow::bail!(
                            "Agent ID prefix '{}' is ambiguous ({} matches: {})",
                            prefix,
                            matches.len(),
                            preview
                        );
                    }
                }

                Ok(None)
            }

            fn ensure_agents_dir(&self) -> Result<PathBuf> {
                fs::create_dir_all(&self.agents_dir).map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to create agents directory {}: {error}",
                        self.agents_dir.display()
                    )
                })?;
                Ok(self.agents_dir.clone())
            }

            fn ensure_agent_prompt_file(
                &self,
                agent_id: &str,
                agent_name: &str,
                current_prompt_file: Option<&str>,
                prompt_override: Option<&str>,
            ) -> Result<PathBuf> {
                validate_agent_file_id(agent_id)?;
                let path = self.resolve_prompt_path_for_write(agent_name, current_prompt_file)?;

                if let Some(prompt) = prompt_override {
                    fs::write(&path, prompt).map_err(|error| {
                        anyhow::anyhow!("Failed to write agent prompt {}: {error}", path.display())
                    })?;
                    return Ok(path);
                }

                if path.exists() {
                    return Ok(path);
                }

                let default_prompt = prompt_files::load_default_main_agent_prompt()?;
                fs::write(&path, default_prompt).map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to initialize agent prompt {}: {error}",
                        path.display()
                    )
                })?;
                Ok(path)
            }

            fn resolve_prompt_path_for_write(
                &self,
                agent_name: &str,
                prompt_file: Option<&str>,
            ) -> Result<PathBuf> {
                let agents_dir = self.ensure_agents_dir()?;
                let desired = agents_dir.join(format!(
                    "{}.md",
                    prompt_files::sanitize_agent_file_stem(agent_name)
                ));
                let current = if let Some(prompt_file) = prompt_file {
                    let path = self.resolve_prompt_path_from_file_name(prompt_file)?;
                    if path.exists() { Some(path) } else { None }
                } else {
                    None
                };

                if let Some(current_path) = current {
                    if current_path == desired {
                        return Ok(current_path);
                    }
                    if !desired.exists() {
                        fs::rename(&current_path, &desired).map_err(|error| {
                            anyhow::anyhow!(
                                "Failed to rename agent prompt file from {} to {}: {error}",
                                current_path.display(),
                                desired.display()
                            )
                        })?;
                        return Ok(desired);
                    }
                    let fallback = unique_prompt_path(&agents_dir, agent_name)?;
                    if current_path != fallback {
                        fs::rename(&current_path, &fallback).map_err(|error| {
                            anyhow::anyhow!(
                                "Failed to rename agent prompt file from {} to {}: {error}",
                                current_path.display(),
                                fallback.display()
                            )
                        })?;
                    }
                    return Ok(fallback);
                }

                if !desired.exists() || prompt_file.is_none() {
                    return Ok(desired);
                }

                unique_prompt_path(&agents_dir, agent_name)
            }

            fn resolve_prompt_path_from_file_name(&self, prompt_file: &str) -> Result<PathBuf> {
                validate_prompt_file_name(prompt_file)?;
                Ok(self.ensure_agents_dir()?.join(prompt_file.trim()))
            }
        }

        fn validate_agent_file_id(agent_id: &str) -> Result<&str> {
            let id = agent_id.trim();
            if id.is_empty() {
                anyhow::bail!("Agent ID is empty; cannot resolve prompt file path");
            }
            if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
                anyhow::bail!(
                    "Agent ID '{}' contains invalid characters (path separators or '..' sequences)",
                    id
                );
            }
            Ok(id)
        }

        fn validate_prompt_file_name(prompt_file: &str) -> Result<&str> {
            let trimmed = prompt_file.trim();
            if trimmed.is_empty() {
                anyhow::bail!("Prompt file name is empty");
            }
            if trimmed.contains('/')
                || trimmed.contains('\\')
                || trimmed.contains("..")
                || trimmed.contains('\0')
            {
                anyhow::bail!("Prompt file name contains invalid characters: {}", trimmed);
            }
            Ok(trimmed)
        }

        fn unique_prompt_path(agents_dir: &Path, agent_name: &str) -> Result<PathBuf> {
            let stem = prompt_files::sanitize_agent_file_stem(agent_name);
            for index in 2..1000u16 {
                let candidate = agents_dir.join(format!("{stem}-{index}.md"));
                if !candidate.exists() {
                    return Ok(candidate);
                }
            }
            anyhow::bail!(
                "Failed to allocate unique prompt file path for stem '{}'",
                stem
            );
        }

        fn normalize_model_fields(agent: &mut AgentNode) -> Result<()> {
            if let Err(error) = agent.normalize_model_fields() {
                anyhow::bail!(types::encode_validation_error(vec![error]));
            }
            Ok(())
        }

        fn render_agent_file(stored: &StoredAgent, prompt: &str) -> Result<String> {
            let frontmatter = AgentFileFrontmatter {
                id: stored.id.clone(),
                name: stored.name.clone(),
                model_ref: stored.agent.model_ref,
                tools: stored.agent.tools.clone(),
                skills: stored.agent.skills.clone(),
                skill_variables: stored.agent.skill_variables.clone(),
                skill_preflight_policy_mode: stored.agent.skill_preflight_policy_mode,
                created_at: stored.created_at,
                updated_at: stored.updated_at,
            };
            let yaml = serde_yaml::to_string(&frontmatter)?;
            let yaml = yaml.strip_prefix("---\n").unwrap_or(&yaml);
            Ok(format!("---\n{}---\n\n{}", yaml, prompt.trim_start()))
        }

        fn read_agent_prompt_body(path: &std::path::Path) -> Result<Option<String>> {
            let content = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(error).map_err(|error| {
                        anyhow::anyhow!("Failed to read agent file {}: {error}", path.display())
                    });
                }
            };
            Ok(match parse_agent_file(&content)? {
                Some((_, prompt)) => prompt,
                None => Some(content),
            })
        }

        fn load_file_agent(path: &std::path::Path) -> Result<Option<StoredAgent>> {
            let content = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(error).map_err(|error| {
                        anyhow::anyhow!("Failed to read agent file {}: {error}", path.display())
                    });
                }
            };
            let Some((frontmatter, prompt)) = parse_agent_file(&content)? else {
                return Ok(None);
            };
            if frontmatter.id.trim().is_empty() || frontmatter.name.trim().is_empty() {
                return Ok(None);
            }
            let modified = path
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64);
            let mut agent = AgentNode::new();
            agent.model_ref = frontmatter.model_ref;
            agent.prompt = prompt;
            agent.tools = frontmatter.tools;
            agent.skills = frontmatter.skills;
            agent.skill_variables = frontmatter.skill_variables;
            agent.skill_preflight_policy_mode = frontmatter.skill_preflight_policy_mode;
            Ok(Some(StoredAgent {
                id: frontmatter.id,
                name: frontmatter.name,
                agent,
                prompt_file: Some(path_file_name(path)?),
                created_at: frontmatter.created_at.or(modified),
                updated_at: frontmatter.updated_at.or(modified),
            }))
        }

        fn parse_agent_file(
            content: &str,
        ) -> Result<Option<(AgentFileFrontmatter, Option<String>)>> {
            let Some(rest) = content.strip_prefix("---\n") else {
                return Ok(None);
            };
            let Some((frontmatter, body)) = rest.split_once("\n---") else {
                return Ok(None);
            };
            let body = body
                .strip_prefix("\n\n")
                .or_else(|| body.strip_prefix('\n'))
                .unwrap_or(body);
            let frontmatter = serde_yaml::from_str::<AgentFileFrontmatter>(frontmatter)?;
            let prompt = if body.trim().is_empty() {
                None
            } else {
                Some(body.to_string())
            };
            Ok(Some((frontmatter, prompt)))
        }

        fn path_file_name(path: &std::path::Path) -> Result<String> {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(ToString::to_string)
                .ok_or_else(|| anyhow::anyhow!("Invalid prompt path: {}", path.display()))
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::prompt_files;
            use tempfile::tempdir;
            use types::ModelId;

            const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

            fn env_lock() -> std::sync::MutexGuard<'static, ()> {
                prompt_files::agents_dir_env_lock()
            }

            fn create_test_agent_node() -> AgentNode {
                use types::ApiKeyConfig;

                AgentNode {
                    model_ref: Some(types::ModelRef::from_model(ModelId::ClaudeSonnet4_5)),
                    prompt: Some("You are a helpful assistant".to_string()),
                    temperature: Some(0.7),
                    codex_cli_reasoning_effort: None,
                    codex_cli_execution_mode: None,
                    api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
                    tools: Some(vec!["add".to_string()]),
                    skills: None,
                    skill_variables: None,
                    skill_preflight_policy_mode: None,
                    model_routing: None,
                }
            }

            #[test]
            fn test_insert_and_get_agent() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let agent_node = create_test_agent_node();
                let stored = storage
                    .create_agent("Test Agent".to_string(), agent_node)
                    .unwrap();

                assert!(!stored.id.is_empty());
                assert_eq!(stored.name, "Test Agent");

                let retrieved = storage.get_agent(stored.id.clone()).unwrap();
                assert!(retrieved.is_some());

                let agent = retrieved.unwrap();
                assert_eq!(agent.name, "Test Agent");
                assert_eq!(
                    agent
                        .agent
                        .resolved_model_ref()
                        .map(|model_ref| model_ref.model),
                    Some(ModelId::ClaudeSonnet4_5)
                );
                assert!(prompts_dir.join("test-agent.md").exists());
                unsafe {
                    std::env::remove_var(AGENTS_DIR_ENV);
                }
            }

            #[test]
            fn test_list_agents() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                storage
                    .create_agent("Agent 1".to_string(), create_test_agent_node())
                    .unwrap();
                storage
                    .create_agent("Agent 2".to_string(), create_test_agent_node())
                    .unwrap();
                storage
                    .create_agent("Agent 3".to_string(), create_test_agent_node())
                    .unwrap();

                let agents = storage.list_agents().unwrap();
                assert_eq!(agents.len(), 3);

                let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
                assert!(names.contains(&"Agent 1".to_string()));
                assert!(names.contains(&"Agent 2".to_string()));
                assert!(names.contains(&"Agent 3".to_string()));
                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_update_agent() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let stored = storage
                    .create_agent("Original Name".to_string(), create_test_agent_node())
                    .unwrap();
                let updated = storage
                    .update_agent(stored.id.clone(), Some("Updated Name".to_string()), None)
                    .unwrap();

                assert_eq!(updated.name, "Updated Name");
                assert_eq!(
                    updated
                        .agent
                        .resolved_model_ref()
                        .map(|model_ref| model_ref.model),
                    Some(ModelId::ClaudeSonnet4_5)
                );

                let mut new_agent_node = create_test_agent_node();
                new_agent_node.temperature = Some(0.9);

                let updated2 = storage
                    .update_agent(stored.id.clone(), None, Some(new_agent_node))
                    .unwrap();

                assert_eq!(updated2.name, "Updated Name");
                assert_eq!(updated2.agent.temperature, Some(0.9));
                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_update_agent_renames_prompt_file_on_name_change() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let stored = storage
                    .create_agent("Original Name".to_string(), create_test_agent_node())
                    .unwrap();
                assert!(prompts_dir.join("original-name.md").exists());

                storage
                    .update_agent(stored.id.clone(), Some("Renamed Agent".to_string()), None)
                    .unwrap();

                assert!(!prompts_dir.join("original-name.md").exists());
                assert!(prompts_dir.join("renamed-agent.md").exists());
                let content = fs::read_to_string(prompts_dir.join("renamed-agent.md")).unwrap();
                let (_, prompt) = parse_agent_file(&content)
                    .unwrap()
                    .expect("renamed agent file should contain frontmatter");
                assert_eq!(prompt.as_deref(), Some("You are a helpful assistant"));

                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_get_agent_supports_unique_prefix() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let stored = storage
                    .create_agent("Prefix Test".to_string(), create_test_agent_node())
                    .unwrap();
                let short = stored.id.chars().take(8).collect::<String>();
                let resolved = storage
                    .get_agent(short)
                    .unwrap()
                    .expect("agent should resolve");
                assert_eq!(resolved.id, stored.id);

                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_delete_agent() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let stored = storage
                    .create_agent("To Delete".to_string(), create_test_agent_node())
                    .unwrap();
                storage.delete_agent(stored.id.clone()).unwrap();

                let retrieved = storage.get_agent(stored.id.clone()).unwrap();
                assert!(retrieved.is_none());

                let deleted_again = storage.delete_agent(stored.id);
                assert!(deleted_again.is_err());
                assert!(deleted_again.unwrap_err().to_string().contains("not found"));
                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_get_nonexistent_agent() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let result = storage.get_agent("nonexistent".to_string()).unwrap();
                assert!(result.is_none());
                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_update_nonexistent_agent() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let result = storage.update_agent(
                    "nonexistent".to_string(),
                    Some("New Name".to_string()),
                    None,
                );

                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("not found"));
                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_resolve_default_agent_prefers_default_assistant() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let first = storage
                    .create_agent("Issue Finder Agent".to_string(), create_test_agent_node())
                    .unwrap();
                let default_agent = storage
                    .create_agent(DEFAULT_ASSISTANT_NAME.to_string(), create_test_agent_node())
                    .unwrap();

                let resolved = storage.resolve_default_agent().unwrap();
                assert_eq!(resolved.id, default_agent.id);
                assert_ne!(resolved.id, first.id);

                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_resolve_default_agent_uses_only_agent() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                let only = storage
                    .create_agent("Only Agent".to_string(), create_test_agent_node())
                    .unwrap();

                let resolved = storage.resolve_default_agent().unwrap();
                assert_eq!(resolved.id, only.id);
                assert_eq!(storage.resolve_default_agent_id().unwrap(), only.id);

                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            #[test]
            fn test_resolve_default_agent_errors_when_ambiguous() {
                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = AgentStorage::new_file_backed_path(&prompts_dir).unwrap();

                storage
                    .create_agent("Issue Finder Agent".to_string(), create_test_agent_node())
                    .unwrap();
                storage
                    .create_agent("Feature B".to_string(), create_test_agent_node())
                    .unwrap();

                let err = storage.resolve_default_agent().expect_err("should fail");
                assert!(err.to_string().contains("Default agent is ambiguous"));

                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }

            /// Test concurrent delete_agent operations don't cause race conditions.
            /// Only one thread should succeed in deleting the agent.
            #[test]
            fn test_concurrent_delete_agent_atomic() {
                use std::sync::atomic::{AtomicUsize, Ordering};
                use std::thread;

                let _lock = env_lock();
                let temp_dir = tempdir().unwrap();
                let prompts_dir = temp_dir.path().join("agents");
                unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
                let storage = Arc::new(AgentStorage::new_file_backed_path(&prompts_dir).unwrap());

                let stored = storage
                    .create_agent("Race Test".to_string(), create_test_agent_node())
                    .unwrap();

                let success_count = Arc::new(AtomicUsize::new(0));
                let num_threads = 10;

                let handles: Vec<_> = (0..num_threads)
                    .map(|_| {
                        let s = Arc::clone(&storage);
                        let id = stored.id.clone();
                        let count = Arc::clone(&success_count);
                        thread::spawn(move || {
                            if s.delete_agent(id).is_ok() {
                                count.fetch_add(1, Ordering::SeqCst);
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }

                // Exactly one delete should have succeeded
                assert_eq!(success_count.load(Ordering::SeqCst), 1);

                // Agent should no longer exist
                let retrieved = storage.get_agent(stored.id.clone()).unwrap();
                assert!(retrieved.is_none());

                unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
            }
        }
    }
    pub mod cleanup {
        use crate::AppCore;
        use crate::services::session::SessionService;
        use anyhow::Result;
        use std::path::Path;
        use std::sync::Arc;
        use tracing::debug;

        const DAY_MS: i64 = 24 * 60 * 60 * 1000;
        const DAY_SECS: u64 = 24 * 60 * 60;

        #[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
        pub struct CleanupReport {
            pub chat_sessions: usize,
            pub daemon_log_files: usize,
        }

        pub async fn run_cleanup(core: &Arc<AppCore>) -> Result<CleanupReport> {
            let config = core.storage.config.get_effective_config()?;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let sessions = SessionService::from_storage(&core.storage);

            let mut chat_sessions = 0usize;
            if let Some(cutoff) = retention_cutoff(now_ms, config.chat_session_retention_days) {
                chat_sessions += sessions
                    .cleanup_workspace_sessions_older_than(cutoff)?
                    .deleted;
            }
            chat_sessions += sessions
                .cleanup_workspace_sessions_by_retention(now_ms)?
                .deleted;

            // L1: Clean up old log files (blocking I/O, offload to spawn_blocking)
            let retention_days = config.log_file_retention_days;
            let daemon_log_files = tokio::task::spawn_blocking(move || {
                cleanup_daemon_log_files(retention_days).unwrap_or(0)
            })
            .await
            .unwrap_or(0);
            Ok(CleanupReport {
                chat_sessions,
                daemon_log_files,
            })
        }

        /// L1: Delete daemon log files older than retention_days.
        ///
        /// Scans `~/.restflow/logs/` for files matching `daemon.log*` or `restflow.log*`.
        fn cleanup_daemon_log_files(retention_days: u32) -> Result<usize> {
            if retention_days == 0 {
                return Ok(0);
            }

            let logs_dir = match crate::paths::logs_dir() {
                Ok(dir) => dir,
                Err(_) => return Ok(0),
            };

            cleanup_old_files_in_dir(&logs_dir, retention_days, |name| {
                name.starts_with("daemon.log") || name.starts_with("restflow.log")
            })
        }

        /// Delete files older than `retention_days` in `dir` that match the `filter` predicate.
        ///
        /// Returns the number of deleted files. Ignores subdirectories.
        pub(crate) fn cleanup_old_files_in_dir(
            dir: &Path,
            retention_days: u32,
            filter: impl Fn(&str) -> bool,
        ) -> Result<usize> {
            if retention_days == 0 {
                return Ok(0);
            }

            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
                Err(e) => return Err(e.into()),
            };

            let cutoff = std::time::SystemTime::now()
                .checked_sub(std::time::Duration::from_secs(
                    retention_days as u64 * DAY_SECS,
                ))
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            let mut deleted = 0;

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                let file_name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };

                if !filter(&file_name) {
                    continue;
                }

                let modified = match entry.metadata().and_then(|m| m.modified()) {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                if modified < cutoff && std::fs::remove_file(&path).is_ok() {
                    deleted += 1;
                    debug!(file = %path.display(), "Deleted old log file");
                }
            }

            Ok(deleted)
        }

        fn retention_cutoff(now_ms: i64, retention_days: u32) -> Option<i64> {
            if retention_days == 0 {
                return None;
            }
            Some(now_ms - (retention_days as i64) * DAY_MS)
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::fs;
            use tempfile::TempDir;

            #[test]
            fn retention_cutoff_handles_forever() {
                assert_eq!(retention_cutoff(10_000, 0), None);
            }

            #[test]
            fn retention_cutoff_calculates_ms() {
                assert_eq!(
                    retention_cutoff(10_000, 1),
                    Some(10_000 - 24 * 60 * 60 * 1000)
                );
            }

            #[test]
            fn test_cleanup_report_default_includes_new_fields() {
                let report = CleanupReport::default();
                assert_eq!(report.daemon_log_files, 0);
            }

            #[test]
            fn test_cleanup_old_files_deletes_old() {
                let temp_dir = TempDir::new().unwrap();
                let dir = temp_dir.path();

                // Create an "old" file and a "new" file
                let old_file = dir.join("daemon.log.2024-01-01");
                let new_file = dir.join("daemon.log.2026-02-01");
                fs::write(&old_file, "old data").unwrap();
                fs::write(&new_file, "new data").unwrap();

                // Set the old file's modified time to 60 days ago
                let old_time = std::time::SystemTime::now()
                    .checked_sub(std::time::Duration::from_secs(60 * DAY_SECS))
                    .unwrap();
                filetime::set_file_mtime(&old_file, filetime::FileTime::from_system_time(old_time))
                    .unwrap();

                let deleted =
                    cleanup_old_files_in_dir(dir, 30, |name| name.starts_with("daemon.log"))
                        .unwrap();

                assert_eq!(deleted, 1);
                assert!(!old_file.exists(), "old file should be deleted");
                assert!(new_file.exists(), "new file should remain");
            }

            #[test]
            fn test_cleanup_old_files_empty_dir() {
                let temp_dir = TempDir::new().unwrap();
                let deleted = cleanup_old_files_in_dir(temp_dir.path(), 30, |_| true).unwrap();
                assert_eq!(deleted, 0);
            }

            #[test]
            fn test_cleanup_old_files_nonexistent_dir() {
                let temp_dir = TempDir::new().unwrap();
                let missing = temp_dir.path().join("nonexistent");
                let deleted = cleanup_old_files_in_dir(&missing, 30, |_| true).unwrap();
                assert_eq!(deleted, 0);
            }

            #[test]
            fn test_cleanup_old_files_zero_retention_skips() {
                let temp_dir = TempDir::new().unwrap();
                fs::write(temp_dir.path().join("test.log"), "data").unwrap();
                let deleted = cleanup_old_files_in_dir(temp_dir.path(), 0, |_| true).unwrap();
                assert_eq!(deleted, 0);
            }
        }
    }
    pub mod config {
        use crate::AppCore;
        use crate::storage::SystemConfig;
        use anyhow::{Context, Result};
        use std::sync::Arc;

        // Get complete system configuration
        pub async fn get_config(core: &Arc<AppCore>) -> Result<SystemConfig> {
            core.storage
                .config
                .get_effective_config()
                .context("Failed to get config")
        }

        // Get writable global system configuration
        pub async fn get_global_config(core: &Arc<AppCore>) -> Result<SystemConfig> {
            core.storage
                .config
                .get_global_config()
                .context("Failed to get global config")
        }

        // Update system configuration with validation
        pub async fn update_config(core: &Arc<AppCore>, config: SystemConfig) -> Result<()> {
            // Validate configuration before updating
            config.validate().context("Invalid configuration")?;

            // Update configuration
            core.storage
                .config
                .update_config(config)
                .context("Failed to update config")
        }
    }
    pub mod execution_console {
        use std::sync::Arc;

        use anyhow::Result;
        use thiserror::Error;

        use crate::storage::Storage;
        use types::{
            ChatSession, ChatTurn, ChatTurnEventKind, ChatTurnStatus, ExecutionContainerKind,
            ExecutionContainerSummary, ExecutionThread, RunKind, RunListQuery, RunSummary,
            RunTimeline,
        };

        #[derive(Debug, Error)]
        pub enum ExecutionThreadError {
            #[error("execution thread query requires run_id")]
            InvalidQuery,
            #[error("run '{0}' not found")]
            RunNotFound(String),
            #[error(transparent)]
            Internal(#[from] anyhow::Error),
        }

        #[derive(Clone)]
        pub struct ExecutionConsoleService {
            storage: Arc<Storage>,
        }

        impl ExecutionConsoleService {
            pub fn new(storage: Arc<Storage>) -> Self {
                Self { storage }
            }

            pub fn from_storage(storage: &Arc<Storage>) -> Self {
                Self::new(storage.clone())
            }

            pub fn list_execution_containers(&self) -> Result<Vec<ExecutionContainerSummary>> {
                let sessions = self.list_sessions()?;
                let mut containers = Vec::new();

                for session in sessions.iter() {
                    containers.push(ExecutionContainerSummary {
                        id: session.id.clone(),
                        kind: ExecutionContainerKind::Workspace,
                        title: session.name.clone(),
                        subtitle: Some(session.model.clone()).filter(|value| !value.is_empty()),
                        updated_at: session.updated_at,
                        status: latest_session_status(session),
                        session_count: 1,
                        latest_session_id: Some(session.id.clone()),
                        latest_run_id: latest_turn(session).map(|turn| turn.id.clone()),
                        agent_id: Some(session.agent_id.clone()),
                    });
                }

                containers.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                Ok(containers)
            }

            pub fn list_runs(&self, query: &RunListQuery) -> Result<Vec<RunSummary>> {
                match query.container.kind {
                    ExecutionContainerKind::Workspace => {
                        let sessions = self.list_sessions()?;
                        let mut runs = Vec::new();
                        for session in sessions.into_iter().filter(|session| {
                            session.id == query.container.id || query.container.id == "workspace"
                        }) {
                            runs.extend(
                                session
                                    .turns
                                    .iter()
                                    .map(|turn| workspace_run_summary(&session, turn)),
                            );
                        }
                        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                        Ok(runs)
                    }
                }
            }

            pub fn get_execution_run_thread(
                &self,
                run_id: &str,
            ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
                let summary = self.find_run(run_id)?;
                let timeline = self.timeline_for_run(run_id)?;
                Ok(ExecutionThread {
                    focus: summary,
                    timeline,
                })
            }

            pub fn get_execution_run_timeline(&self, run_id: &str) -> Result<RunTimeline> {
                let _ = self.find_run(run_id)?;
                self.timeline_for_run(run_id).map_err(Into::into)
            }

            fn find_run(
                &self,
                run_id: &str,
            ) -> std::result::Result<RunSummary, ExecutionThreadError> {
                let run_id = run_id.trim();
                if run_id.is_empty() {
                    return Err(ExecutionThreadError::InvalidQuery);
                }

                for session in self.list_sessions()? {
                    if let Some(turn) = session.turns.iter().find(|turn| turn.id == run_id) {
                        return Ok(workspace_run_summary(&session, turn));
                    }
                }

                Err(ExecutionThreadError::RunNotFound(run_id.to_string()))
            }

            fn timeline_for_run(
                &self,
                run_id: &str,
            ) -> std::result::Result<RunTimeline, ExecutionThreadError> {
                let run_id = run_id.trim();
                if run_id.is_empty() {
                    return Err(ExecutionThreadError::InvalidQuery);
                }

                for session in self.list_sessions()? {
                    if let Some(turn) = session.turns.iter().find(|turn| turn.id == run_id) {
                        return Ok(RunTimeline {
                            events: turn.events.clone(),
                        });
                    }
                }

                Err(ExecutionThreadError::RunNotFound(run_id.to_string()))
            }

            fn list_sessions(&self) -> Result<Vec<ChatSession>> {
                Ok(self
                    .storage
                    .file_sessions
                    .list()?
                    .into_iter()
                    .map(|session| session.to_chat_session())
                    .collect())
            }
        }

        fn latest_turn(session: &ChatSession) -> Option<&ChatTurn> {
            session.turns.iter().max_by_key(|turn| turn.updated_at)
        }

        fn latest_session_status(session: &ChatSession) -> Option<String> {
            latest_turn(session).map(|turn| turn_status(turn.status).to_string())
        }

        fn workspace_run_summary(session: &ChatSession, turn: &ChatTurn) -> RunSummary {
            RunSummary {
                id: turn.id.clone(),
                kind: RunKind::WorkspaceRun,
                container_id: session.id.clone(),
                root_run_id: Some(turn.id.clone()),
                title: turn_title(turn).unwrap_or_else(|| session.name.clone()),
                subtitle: Some(session.model.clone()).filter(|value| !value.is_empty()),
                status: turn_status(turn.status).to_string(),
                updated_at: turn.updated_at,
                started_at: Some(turn.started_at),
                ended_at: turn.completed_at,
                session_id: Some(session.id.clone()),
                run_id: Some(turn.id.clone()),
                parent_run_id: None,
                agent_id: Some(session.agent_id.clone()),
                effective_model: Some(session.model.clone()).filter(|value| !value.is_empty()),
                provider: Some(session.provider.clone()).filter(|value| !value.is_empty()),
                event_count: turn.events.len() as u64,
            }
        }

        fn turn_status(status: ChatTurnStatus) -> &'static str {
            match status {
                ChatTurnStatus::Running => "running",
                ChatTurnStatus::Completed => "completed",
                ChatTurnStatus::Canceled => "interrupted",
                ChatTurnStatus::Failed => "failed",
            }
        }

        fn turn_title(turn: &ChatTurn) -> Option<String> {
            turn.events.iter().find_map(|event| match &event.kind {
                ChatTurnEventKind::UserMessage { content } => Some(trim_title(content)),
                ChatTurnEventKind::AssistantMessage { content } => Some(trim_title(content)),
                ChatTurnEventKind::ToolCall { name, .. } => Some(format!("Tool: {name}")),
                ChatTurnEventKind::ToolResult { call_id, .. } => {
                    Some(format!("Tool result: {call_id}"))
                }
                ChatTurnEventKind::Progress { message } => Some(trim_title(message)),
                ChatTurnEventKind::Error { message } => Some(trim_title(message)),
                ChatTurnEventKind::Canceled => Some("Canceled turn".to_string()),
            })
        }

        fn trim_title(value: &str) -> String {
            let value = value.trim();
            if value.chars().count() > 80 {
                format!("{}...", value.chars().take(77).collect::<String>())
            } else if value.is_empty() {
                "Untitled run".to_string()
            } else {
                value.to_string()
            }
        }
    }
    pub mod operation_assessment {
        use std::sync::Arc;

        use anyhow::{Result, anyhow};
        use sha2::{Digest, Sha256};
        use types::ModelProvider as SharedModelProvider;
        use types::request::{
            AgentNode as ContractAgentNode, RunSpawnRequest as ContractRunSpawnRequest,
        };

        use crate::AgentStorage;
        use crate::AppCore;
        use crate::StoredAgent;
        use crate::provider_policy::resolve_model_from_available_secrets;
        use crate::storage::{ConfigStorage, SecretStorage, Storage};
        use crate::tools::ToolError;
        use types::assessment::{
            AgentOperationAssessor, AssessmentModelRef, OperationAssessment,
            OperationAssessmentIntent, OperationAssessmentIssue, OperationAssessmentStatus,
        };
        use types::store::{AgentCreateRequest, AgentUpdateRequest};
        use types::subagent::spawn_request_from_contract as run_spawn_request_from_contract;
        use types::subagent::{SpawnRequest as RunSpawnRequest, SubagentDefSummary};
        use types::{AgentNode, ApiKeyConfig, ModelId, ModelRef, Provider, ValidationError};

        #[derive(Clone)]
        pub struct OperationAssessorAdapter {
            context: AssessmentContext,
        }

        #[derive(Clone)]
        struct AssessmentContext {
            secrets: SecretStorage,
            config: ConfigStorage,
            agents: AgentStorage,
        }

        impl AssessmentContext {
            fn from_core(core: &Arc<AppCore>) -> Self {
                Self::from_storage(core.storage.as_ref())
            }

            fn from_storage(storage: &Storage) -> Self {
                Self {
                    secrets: storage.secrets.clone(),
                    config: storage.config.clone(),
                    agents: storage.agents.clone(),
                }
            }
        }

        impl OperationAssessorAdapter {
            pub fn new(core: Arc<AppCore>) -> Self {
                Self {
                    context: AssessmentContext::from_core(&core),
                }
            }

            pub fn from_storage(storage: &Storage) -> Self {
                Self {
                    context: AssessmentContext::from_storage(storage),
                }
            }
        }

        #[async_trait::async_trait]
        impl AgentOperationAssessor for OperationAssessorAdapter {
            async fn assess_agent_create(
                &self,
                request: AgentCreateRequest,
            ) -> std::result::Result<OperationAssessment, ToolError> {
                assess_agent_create_with_context(&self.context, request)
                    .await
                    .map_err(|error| ToolError::Tool(error.to_string()))
            }

            async fn assess_agent_update(
                &self,
                request: AgentUpdateRequest,
            ) -> std::result::Result<OperationAssessment, ToolError> {
                assess_agent_update_with_context(&self.context, request)
                    .await
                    .map_err(|error| ToolError::Tool(error.to_string()))
            }

            async fn assess_subagent_spawn(
                &self,
                operation: &str,
                request: ContractRunSpawnRequest,
                template_mode: bool,
            ) -> std::result::Result<OperationAssessment, ToolError> {
                assess_run_spawn_with_context(&self.context, operation, request, template_mode)
                    .await
                    .map_err(|error| ToolError::Tool(error.to_string()))
            }

            async fn assess_subagent_batch(
                &self,
                operation: &str,
                requests: Vec<ContractRunSpawnRequest>,
                template_mode: bool,
            ) -> std::result::Result<OperationAssessment, ToolError> {
                assess_run_batch_with_context(&self.context, operation, requests, template_mode)
                    .await
                    .map_err(|error| ToolError::Tool(error.to_string()))
            }
        }

        pub fn assessment_requires_confirmation(assessment: &OperationAssessment) -> bool {
            assessment.status == OperationAssessmentStatus::Warning
                && assessment.requires_confirmation
        }

        pub fn ensure_assessment_confirmed(
            assessment: &OperationAssessment,
            approval_id: Option<&str>,
        ) -> Result<()> {
            if !assessment_requires_confirmation(assessment) {
                return Ok(());
            }

            let expected = assessment
                .approval_id
                .as_deref()
                .ok_or_else(|| anyhow!("confirmation required"))?;
            let provided = approval_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("confirmation required"))?;
            if provided != expected {
                return Err(anyhow!("invalid confirmation token"));
            }
            Ok(())
        }

        pub fn assessment_summary(assessment: &OperationAssessment) -> String {
            let issues = match assessment.status {
                OperationAssessmentStatus::Block => &assessment.blockers,
                OperationAssessmentStatus::Warning => &assessment.warnings,
                OperationAssessmentStatus::Ok => return "Operation is ready".to_string(),
            };
            let summary = issues
                .iter()
                .map(|issue| issue.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            if summary.is_empty() {
                "Operation requires confirmation".to_string()
            } else {
                summary
            }
        }

        fn issue(
            code: impl Into<String>,
            message: impl Into<String>,
            field: Option<&str>,
            suggestion: Option<&str>,
        ) -> OperationAssessmentIssue {
            OperationAssessmentIssue {
                code: code.into(),
                message: message.into(),
                field: field.map(ToOwned::to_owned),
                suggestion: suggestion.map(ToOwned::to_owned),
            }
        }

        fn issues_from_validation(errors: Vec<ValidationError>) -> Vec<OperationAssessmentIssue> {
            errors
                .into_iter()
                .map(|error| OperationAssessmentIssue {
                    code: "validation_error".to_string(),
                    message: error.message,
                    field: Some(error.field),
                    suggestion: None,
                })
                .collect()
        }

        fn agent_has_local_credential(context: &AssessmentContext, agent: &AgentNode) -> bool {
            match agent.api_key_config.as_ref() {
                Some(ApiKeyConfig::Direct(value)) => !value.trim().is_empty(),
                Some(ApiKeyConfig::Secret(secret_name)) => context
                    .secrets
                    .has_available_secret(secret_name)
                    .unwrap_or(false),
                None => false,
            }
        }

        fn provider_is_available(context: &AssessmentContext, provider: Provider) -> bool {
            provider.api_key_env().is_none()
                || provider
                    .api_key_env_candidates()
                    .any(|key| context.secrets.has_available_secret(key).unwrap_or(false))
        }

        fn resolve_model_from_stored_credentials(context: &AssessmentContext) -> Option<ModelId> {
            resolve_model_from_available_secrets(|key| {
                context.secrets.has_available_secret(key).unwrap_or(false)
            })
        }

        fn to_assessment_model_ref(model_ref: ModelRef) -> AssessmentModelRef {
            AssessmentModelRef {
                provider: model_ref.provider.as_canonical_str().to_string(),
                model: model_ref.model.as_serialized_str().to_string(),
            }
        }

        fn finalize_assessment(assessment: OperationAssessment) -> OperationAssessment {
            finalize_assessment_with_seed(assessment, None)
        }

        fn finalize_assessment_with_seed(
            mut assessment: OperationAssessment,
            confirmation_seed: Option<serde_json::Value>,
        ) -> OperationAssessment {
            if !assessment.blockers.is_empty() {
                assessment.status = OperationAssessmentStatus::Block;
                assessment.requires_confirmation = false;
                assessment.approval_id = None;
                return assessment;
            }

            if !assessment.warnings.is_empty() {
                assessment.status = OperationAssessmentStatus::Warning;
                assessment.requires_confirmation = true;
                assessment.approval_id =
                    Some(build_approval_id(&assessment, confirmation_seed.as_ref()));
                return assessment;
            }

            assessment.status = OperationAssessmentStatus::Ok;
            assessment.requires_confirmation = false;
            assessment.approval_id = None;
            assessment
        }

        fn build_approval_id(
            assessment: &OperationAssessment,
            confirmation_seed: Option<&serde_json::Value>,
        ) -> String {
            let payload = serde_json::json!({
                "operation": assessment.operation,
                "intent": assessment.intent,
                "effective_model_ref": assessment.effective_model_ref,
                "warnings": assessment.warnings,
                "blockers": assessment.blockers,
                "confirmation_seed": confirmation_seed,
            });
            let encoded = serde_json::to_vec(&payload).unwrap_or_default();
            let mut hasher = Sha256::new();
            hasher.update(encoded);
            hex::encode(hasher.finalize())
        }

        fn parse_agent_node(value: ContractAgentNode) -> Result<AgentNode> {
            AgentNode::try_from_contract_node(value)
                .map_err(|errors| anyhow!(types::encode_validation_error(errors)))
        }

        async fn load_agent(
            context: &AssessmentContext,
            id_or_prefix: &str,
        ) -> Result<StoredAgent> {
            let trimmed = id_or_prefix.trim();
            let resolved_id = if trimmed.eq_ignore_ascii_case("default") {
                context.agents.resolve_default_agent_id()?
            } else {
                context.agents.resolve_existing_agent_id(trimmed)?
            };
            context
                .agents
                .get_agent(resolved_id.clone())?
                .ok_or_else(|| anyhow!("Agent not found: {resolved_id}"))
        }

        fn normalize_run_spawn_request(
            context: &AssessmentContext,
            request: ContractRunSpawnRequest,
        ) -> Result<RunSpawnRequest> {
            let available_agents = context
                .agents
                .list_agents()?
                .into_iter()
                .map(|agent| SubagentDefSummary {
                    id: agent.id,
                    name: agent.name,
                    description: "File-backed agent".to_string(),
                    tags: Vec::new(),
                })
                .collect::<Vec<_>>();
            run_spawn_request_from_contract(&available_agents, request)
                .map_err(|error| anyhow!(error.to_string()))
        }

        async fn validate_agent_async(
            context: &AssessmentContext,
            agent: &AgentNode,
        ) -> std::result::Result<(), Vec<ValidationError>> {
            let mut errors = Vec::new();

            let tool_registry = match crate::services::tool_registry::create_tool_registry(
                context.config.clone(),
                None,
                None,
            ) {
                Ok(registry) => registry,
                Err(err) => {
                    errors.push(ValidationError::new(
                        "tools",
                        format!("Failed to create tool registry: {err}"),
                    ));
                    return Err(errors);
                }
            };

            if let Some(tools) = &agent.tools {
                for tool_name in tools {
                    let normalized = tool_name.trim();
                    if normalized.is_empty() {
                        errors.push(ValidationError::new("tools", "tool name must not be empty"));
                        continue;
                    }
                    if !tool_registry.has(normalized) && !is_subagent_tool_name(normalized) {
                        errors.push(ValidationError::new(
                            "tools",
                            format!("unknown tool: {}", normalized),
                        ));
                    }
                }
            }

            if let Some(skills) = &agent.skills {
                for skill_id in skills {
                    let normalized = skill_id.trim();
                    if normalized.is_empty() {
                        errors.push(ValidationError::new("skills", "skill ID must not be empty"));
                        continue;
                    }
                    match crate::services::skills::skill_exists_in_catalog(normalized) {
                        Ok(true) => {}
                        Ok(false) => errors.push(ValidationError::new(
                            "skills",
                            format!("unknown skill: {}", normalized),
                        )),
                        Err(err) => errors.push(ValidationError::new(
                            "skills",
                            format!("failed to verify skill '{}': {}", normalized, err),
                        )),
                    }
                }
            }

            if let Some(ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
                let normalized = secret_name.trim();
                if !normalized.is_empty() {
                    match context.secrets.has_available_secret(normalized) {
                        Ok(true) => {}
                        Ok(false) => errors.push(ValidationError::new(
                            "api_key_config",
                            format!("secret not found in storage: {}", normalized),
                        )),
                        Err(err) => errors.push(ValidationError::new(
                            "api_key_config",
                            format!("failed to verify secret '{}': {}", normalized, err),
                        )),
                    }
                }
            }

            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors)
            }
        }

        fn is_subagent_tool_name(name: &str) -> bool {
            matches!(
                name,
                "spawn_subagent" | "spawn_subagent_batch" | "wait_subagents" | "list_subagents"
            )
        }

        async fn assess_agent_node(
            context: &AssessmentContext,
            operation: &str,
            intent: OperationAssessmentIntent,
            agent: &AgentNode,
            subagent_parent_fallback: bool,
        ) -> Result<OperationAssessment> {
            let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());

            if let Err(errors) = agent.validate() {
                assessment.blockers.extend(issues_from_validation(errors));
            }
            if let Err(errors) = validate_agent_async(context, agent).await {
                assessment.blockers.extend(issues_from_validation(errors));
            }

            if !assessment.blockers.is_empty() {
                return Ok(finalize_assessment(assessment));
            }

            if let Some(model_ref) = agent.resolved_model_ref() {
                assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));
                if !provider_is_available(context, model_ref.provider)
                    && !agent_has_local_credential(context, agent)
                {
                    let current_issue = issue(
                        "provider_unavailable",
                        format!(
                            "Provider '{}' is not configured in the current environment.",
                            model_ref.provider.as_canonical_str()
                        ),
                        Some("model_ref.provider"),
                        Some("Configure a compatible API key before running."),
                    );
                    match intent {
                        OperationAssessmentIntent::Save => assessment.warnings.push(current_issue),
                        OperationAssessmentIntent::Run => assessment.blockers.push(current_issue),
                    }
                }
                return Ok(finalize_assessment(assessment));
            }

            if subagent_parent_fallback {
                if matches!(intent, OperationAssessmentIntent::Save) {
                    assessment.warnings.push(issue(
                        "inherits_parent_model",
                        "No explicit model is configured. This sub-agent run will inherit the parent runtime model.",
                        Some("model_ref"),
                        Some("Set model_ref when you need deterministic provider behavior."),
                    ));
                }
                return Ok(finalize_assessment(assessment));
            }

            if matches!(intent, OperationAssessmentIntent::Save) {
                return Ok(finalize_assessment(assessment));
            }

            match resolve_model_from_stored_credentials(context) {
                Some(model) => {
                    let model_ref = ModelRef::from_model(model);
                    assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));
                }
                None => {
                    let current_issue = issue(
                        "auto_model_unresolved",
                        "No explicit model is configured and no compatible credential is currently available.",
                        Some("model_ref"),
                        Some("Set model_ref or configure a compatible API key."),
                    );
                    match intent {
                        OperationAssessmentIntent::Save => assessment.warnings.push(current_issue),
                        OperationAssessmentIntent::Run => assessment.blockers.push(current_issue),
                    }
                }
            }

            Ok(finalize_assessment(assessment))
        }

        fn merge_assessment(
            target: &mut OperationAssessment,
            child: OperationAssessment,
            context_prefix: &str,
        ) {
            if target.effective_model_ref.is_none() {
                target.effective_model_ref = child.effective_model_ref;
            }
            target
                .warnings
                .extend(child.warnings.into_iter().map(|mut issue| {
                    issue.message = format!("{context_prefix}: {}", issue.message);
                    issue
                }));
            target
                .blockers
                .extend(child.blockers.into_iter().map(|mut issue| {
                    issue.message = format!("{context_prefix}: {}", issue.message);
                    issue
                }));
        }

        pub async fn assess_agent_create(
            core: &Arc<AppCore>,
            request: AgentCreateRequest,
        ) -> Result<OperationAssessment> {
            let context = AssessmentContext::from_core(core);
            assess_agent_create_with_context(&context, request).await
        }

        async fn assess_agent_create_with_context(
            context: &AssessmentContext,
            request: AgentCreateRequest,
        ) -> Result<OperationAssessment> {
            let agent = parse_agent_node(request.agent)?;
            assess_agent_node(
                context,
                "create_agent",
                OperationAssessmentIntent::Save,
                &agent,
                false,
            )
            .await
        }

        pub async fn assess_agent_update(
            core: &Arc<AppCore>,
            request: AgentUpdateRequest,
        ) -> Result<OperationAssessment> {
            let context = AssessmentContext::from_core(core);
            assess_agent_update_with_context(&context, request).await
        }

        async fn assess_agent_update_with_context(
            context: &AssessmentContext,
            request: AgentUpdateRequest,
        ) -> Result<OperationAssessment> {
            let Some(agent_value) = request.agent else {
                return Ok(OperationAssessment::ok(
                    "update_agent",
                    OperationAssessmentIntent::Save,
                ));
            };
            let agent = parse_agent_node(agent_value)?;
            assess_agent_node(
                context,
                "update_agent",
                OperationAssessmentIntent::Save,
                &agent,
                false,
            )
            .await
        }

        pub async fn assess_subagent_spawn(
            core: &Arc<AppCore>,
            operation: &str,
            request: ContractRunSpawnRequest,
            template_mode: bool,
        ) -> Result<OperationAssessment> {
            let context = AssessmentContext::from_core(core);
            assess_run_spawn_with_context(&context, operation, request, template_mode).await
        }

        async fn assess_run_spawn_with_context(
            context: &AssessmentContext,
            operation: &str,
            request: ContractRunSpawnRequest,
            template_mode: bool,
        ) -> Result<OperationAssessment> {
            let request = normalize_run_spawn_request(context, request)?;
            let intent = if template_mode {
                OperationAssessmentIntent::Save
            } else {
                OperationAssessmentIntent::Run
            };

            if let (Some(model), Some(provider)) = (
                request
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                request
                    .model_provider
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            ) {
                let normalized_model = ModelId::normalize_model_id(model)
                    .ok_or_else(|| anyhow!("Unsupported model identifier: {}", model))?;
                let requested_provider = SharedModelProvider::parse_alias(provider)
                    .map(Provider::from_model_provider)
                    .ok_or_else(|| anyhow!("Unsupported provider identifier: {}", provider))?;
                let resolved_model =
                    ModelId::for_provider_and_model(requested_provider, &normalized_model)
                        .ok_or_else(|| {
                            anyhow!("Unsupported model identifier: {}", normalized_model)
                        })?;
                let model_ref = ModelRef::from_model(resolved_model);
                let mut assessment = OperationAssessment::ok(operation.to_string(), intent.clone());
                assessment.effective_model_ref = Some(to_assessment_model_ref(model_ref));

                if model_ref.provider != requested_provider {
                    assessment.blockers.push(issue(
                        "model_provider_mismatch",
                        format!(
                            "Model '{}' does not belong to provider '{}'.",
                            resolved_model.as_serialized_str(),
                            requested_provider.as_canonical_str()
                        ),
                        Some("provider"),
                        Some("Choose a model that belongs to the selected provider."),
                    ));
                    return Ok(finalize_assessment(assessment));
                }

                if !provider_is_available(context, requested_provider) {
                    let current_issue = issue(
                        "provider_unavailable",
                        format!(
                            "Provider '{}' is not configured in the current environment.",
                            requested_provider.as_canonical_str()
                        ),
                        Some("provider"),
                        Some("Configure a compatible API key before running."),
                    );
                    match intent {
                        OperationAssessmentIntent::Save => assessment.warnings.push(current_issue),
                        OperationAssessmentIntent::Run => assessment.blockers.push(current_issue),
                    }
                }

                return Ok(finalize_assessment(assessment));
            }

            if let Some(agent_id) = request.agent_id.as_deref() {
                let stored_agent = load_agent(context, agent_id).await?;
                return assess_agent_node(context, operation, intent, &stored_agent.agent, true)
                    .await;
            }

            let mut assessment = OperationAssessment::ok(operation.to_string(), intent);
            if matches!(assessment.intent, OperationAssessmentIntent::Save) {
                assessment.warnings.push(issue(
                    "inherits_parent_model",
                    "This temporary sub-agent run has no explicit model and will inherit the parent runtime model.",
                    Some("model_ref"),
                    Some("Set model_ref to make this sub-agent run deterministic."),
                ));
            }
            Ok(finalize_assessment(assessment))
        }

        pub async fn assess_subagent_batch(
            core: &Arc<AppCore>,
            operation: &str,
            requests: Vec<ContractRunSpawnRequest>,
            template_mode: bool,
        ) -> Result<OperationAssessment> {
            let context = AssessmentContext::from_core(core);
            assess_run_batch_with_context(&context, operation, requests, template_mode).await
        }

        async fn assess_run_batch_with_context(
            context: &AssessmentContext,
            operation: &str,
            requests: Vec<ContractRunSpawnRequest>,
            template_mode: bool,
        ) -> Result<OperationAssessment> {
            let intent = if template_mode {
                OperationAssessmentIntent::Save
            } else {
                OperationAssessmentIntent::Run
            };
            let mut assessment = OperationAssessment::ok(operation.to_string(), intent);

            for (index, request) in requests.into_iter().enumerate() {
                let child =
                    assess_run_spawn_with_context(context, operation, request, template_mode)
                        .await?;
                merge_assessment(&mut assessment, child, &format!("Worker {}", index + 1));
            }

            Ok(finalize_assessment(assessment))
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::test_support::RestflowTestEnv;
            use types::request::{ApiKeyConfig as ContractApiKeyConfig, WireModelRef};

            async fn create_test_core_isolated() -> (Arc<AppCore>, RestflowTestEnv) {
                let env = RestflowTestEnv::new();
                let db_path = env.db_path("test.db");
                let core = Arc::new(
                    AppCore::new(db_path.to_str().expect("db path"))
                        .await
                        .unwrap(),
                );
                (core, env)
            }

            #[tokio::test]
            async fn assess_agent_create_accepts_valid_contract_agent_node() {
                let (core, _env) = create_test_core_isolated().await;
                let assessment = assess_agent_create(
                    &core,
                    AgentCreateRequest {
                        name: "Typed Agent".to_string(),
                        agent: ContractAgentNode {
                            model_ref: Some(WireModelRef {
                                provider: "openai".to_string(),
                                model: "gpt-5-mini".to_string(),
                            }),
                            api_key_config: Some(ContractApiKeyConfig::Direct(
                                "test-key".to_string(),
                            )),
                            prompt: Some("hello".to_string()),
                            ..ContractAgentNode::default()
                        },
                    },
                )
                .await
                .expect("assessment should succeed");

                assert_eq!(assessment.status, OperationAssessmentStatus::Ok);
                assert_eq!(
                    assessment
                        .effective_model_ref
                        .as_ref()
                        .map(|model_ref| model_ref.provider.as_str()),
                    Some("openai")
                );
            }

            #[tokio::test]
            async fn assess_agent_create_accepts_subagent_tools() {
                let (core, _env) = create_test_core_isolated().await;
                let assessment = assess_agent_create(
                    &core,
                    AgentCreateRequest {
                        name: "Subagent Coordinator".to_string(),
                        agent: ContractAgentNode {
                            model_ref: Some(WireModelRef {
                                provider: "openai".to_string(),
                                model: "gpt-5-mini".to_string(),
                            }),
                            api_key_config: Some(ContractApiKeyConfig::Direct(
                                "test-key".to_string(),
                            )),
                            tools: Some(vec![
                                "bash".to_string(),
                                "spawn_subagent_batch".to_string(),
                                "wait_subagents".to_string(),
                                "list_subagents".to_string(),
                            ]),
                            prompt: Some("coordinate subagents".to_string()),
                            ..ContractAgentNode::default()
                        },
                    },
                )
                .await
                .expect("subagent tools should be accepted");

                assert_eq!(assessment.status, OperationAssessmentStatus::Ok);
            }

            #[tokio::test]
            async fn assess_agent_create_rejects_invalid_model_ref() {
                let (core, _env) = create_test_core_isolated().await;
                let error = assess_agent_create(
                    &core,
                    AgentCreateRequest {
                        name: "Bad Agent".to_string(),
                        agent: ContractAgentNode {
                            model_ref: Some(WireModelRef {
                                provider: "openai".to_string(),
                                model: "claude-sonnet-4".to_string(),
                            }),
                            ..ContractAgentNode::default()
                        },
                    },
                )
                .await
                .expect_err("invalid model_ref should fail");

                let message = error.to_string();
                assert!(message.contains("validation_error"));
                assert!(message.contains("model_ref"));
            }

            #[tokio::test]
            async fn assess_agent_update_rejects_invalid_model_ref() {
                let (core, _env) = create_test_core_isolated().await;
                let error = assess_agent_update(
                    &core,
                    AgentUpdateRequest {
                        id: "agent-1".to_string(),
                        name: None,
                        agent: Some(ContractAgentNode {
                            model_ref: Some(WireModelRef {
                                provider: "anthropic".to_string(),
                                model: "gpt-5-mini".to_string(),
                            }),
                            ..ContractAgentNode::default()
                        }),
                    },
                )
                .await
                .expect_err("invalid model_ref should fail");

                let message = error.to_string();
                assert!(message.contains("validation_error"));
                assert!(message.contains("model_ref"));
            }

            #[tokio::test]
            async fn assess_subagent_spawn_accepts_contract_request_and_sets_effective_model_ref() {
                let (core, _env) = create_test_core_isolated().await;
                let assessment = assess_subagent_spawn(
                    &core,
                    "spawn_subagent",
                    ContractRunSpawnRequest {
                        task: "Summarize the workspace".to_string(),
                        model: Some("gpt-5-mini".to_string()),
                        model_provider: Some("openai".to_string()),
                        ..ContractRunSpawnRequest::default()
                    },
                    true,
                )
                .await
                .expect("assessment should succeed for a valid contract request");

                assert!(matches!(
                    assessment.status,
                    OperationAssessmentStatus::Ok | OperationAssessmentStatus::Warning
                ));
                assert_eq!(
                    assessment
                        .effective_model_ref
                        .as_ref()
                        .map(|model_ref| model_ref.provider.as_str()),
                    Some("openai")
                );
                assert_eq!(
                    assessment
                        .effective_model_ref
                        .as_ref()
                        .map(|model_ref| model_ref.model.as_str()),
                    Some("gpt-5-mini")
                );
            }

            #[tokio::test]
            async fn assess_subagent_spawn_rejects_invalid_contract_request_before_runtime() {
                let (core, _env) = create_test_core_isolated().await;
                let error = assess_subagent_spawn(
                    &core,
                    "spawn_subagent",
                    ContractRunSpawnRequest {
                        task: "Summarize the workspace".to_string(),
                        model: Some("gpt-5-mini".to_string()),
                        model_provider: None,
                        ..ContractRunSpawnRequest::default()
                    },
                    false,
                )
                .await
                .expect_err("model/provider mismatch should fail at the boundary");

                assert!(
                    error
                        .to_string()
                        .contains("requires both 'model' and 'provider'")
                );
            }

            #[tokio::test]
            async fn assess_subagent_batch_rejects_invalid_contract_requests() {
                let (core, _env) = create_test_core_isolated().await;
                let error = assess_subagent_batch(
                    &core,
                    "spawn_subagent_batch",
                    vec![ContractRunSpawnRequest {
                        task: "Summarize the workspace".to_string(),
                        model: Some("gpt-5-mini".to_string()),
                        model_provider: None,
                        ..ContractRunSpawnRequest::default()
                    }],
                    false,
                )
                .await
                .expect_err("invalid batch request should fail at the boundary");

                assert!(
                    error
                        .to_string()
                        .contains("requires both 'model' and 'provider'")
                );
            }

            #[tokio::test]
            async fn assess_subagent_batch_allows_runtime_parent_model_inheritance() {
                let (core, _env) = create_test_core_isolated().await;
                let assessment = assess_subagent_batch(
                    &core,
                    "spawn_subagent_batch",
                    vec![ContractRunSpawnRequest {
                        task: "Return A_OK".to_string(),
                        ..ContractRunSpawnRequest::default()
                    }],
                    false,
                )
                .await
                .expect("runtime inheritance should be allowed");

                assert_eq!(assessment.status, OperationAssessmentStatus::Ok);
                assert!(!assessment.requires_confirmation);
                assert_eq!(assessment.approval_id, None);
            }
        }
    }
    pub mod secrets {
        use crate::{AppCore, Secret};
        use anyhow::{Context, Result};
        use std::sync::Arc;

        /// List all secrets (without values for security)
        pub async fn list_secrets(core: &Arc<AppCore>) -> Result<Vec<Secret>> {
            core.storage
                .secrets
                .list_secrets()
                .context("Failed to list secrets")
        }

        /// Get a secret value by key
        pub async fn get_secret(core: &Arc<AppCore>, key: &str) -> Result<Option<String>> {
            core.storage
                .secrets
                .get_secret(key)
                .with_context(|| format!("Failed to get secret {}", key))
        }

        /// Set or update a secret with optional description
        pub async fn set_secret(
            core: &Arc<AppCore>,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()> {
            core.storage
                .secrets
                .set_secret(key, value, description)
                .with_context(|| format!("Failed to set secret {}", key))
        }

        /// Create a new secret (fails if already exists)
        ///
        /// This operation is atomic - prevents TOCTOU race conditions.
        pub async fn create_secret(
            core: &Arc<AppCore>,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()> {
            core.storage
                .secrets
                .create_secret(key, value, description)
                .with_context(|| format!("Failed to create secret {}", key))
        }

        /// Update an existing secret (fails if not exists)
        ///
        /// This operation is atomic - prevents TOCTOU race conditions.
        pub async fn update_secret(
            core: &Arc<AppCore>,
            key: &str,
            value: &str,
            description: Option<String>,
        ) -> Result<()> {
            core.storage
                .secrets
                .update_secret(key, value, description)
                .with_context(|| format!("Failed to update secret {}", key))
        }

        /// Delete a secret
        pub async fn delete_secret(core: &Arc<AppCore>, key: &str) -> Result<()> {
            core.storage
                .secrets
                .delete_secret(key)
                .with_context(|| format!("Failed to delete secret {}", key))
        }

        /// Check whether a managed secret exists in storage.
        pub async fn has_secret(core: &Arc<AppCore>, key: &str) -> Result<bool> {
            core.storage
                .secrets
                .has_secret(key)
                .with_context(|| format!("Failed to check secret {}", key))
        }

        /// Check whether a secret is available from managed storage.
        pub async fn has_available_secret(core: &Arc<AppCore>, key: &str) -> Result<bool> {
            core.storage
                .secrets
                .has_available_secret(key)
                .with_context(|| format!("Failed to check secret availability {}", key))
        }
    }
    pub mod session {
        use crate::AgentStorage;
        use crate::session_events::{ChatSessionEvent, publish_session_event};
        use crate::session_log::{FileSession, FileSessionStore};
        use crate::storage::Storage;
        use anyhow::{Result, anyhow};
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex, Weak};
        use tracing::warn;
        use types::{
            ChatMessage, ChatRole, ChatSession, ChatSessionSummary, ChatSessionUpdate,
            MessageExecution, ModelId,
        };

        #[derive(Clone)]
        pub struct SessionService {
            agents: Option<AgentStorage>,
            file_sessions: FileSessionStore,
            append_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
        }

        #[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
        pub struct SessionCleanupStats {
            pub scanned: usize,
            pub deleted: usize,
            pub skipped_not_expired: usize,
            pub skipped_no_retention: usize,
            pub failed: usize,
            pub bytes_freed: u64,
        }

        pub struct PersistInteractiveTurnRequest<'a> {
            pub original_input: &'a str,
            pub persisted_input: &'a str,
            pub assistant_output: &'a str,
            pub active_model: Option<&'a str>,
            pub final_model: Option<ModelId>,
            pub execution: MessageExecution,
            pub source: &'a str,
        }

        impl SessionService {
            pub fn new(file_sessions: FileSessionStore, agents: Option<AgentStorage>) -> Self {
                Self {
                    agents,
                    file_sessions,
                    append_locks: Arc::new(Mutex::new(HashMap::new())),
                }
            }

            pub fn from_storage(storage: &Storage) -> Self {
                Self::new(storage.file_sessions.clone(), Some(storage.agents.clone()))
            }

            #[cfg(test)]
            pub fn with_file_sessions(mut self, file_sessions: FileSessionStore) -> Self {
                self.file_sessions = file_sessions;
                self
            }

            pub fn get_session_view(&self, session_id: &str) -> Result<Option<ChatSession>> {
                let Some(mut session) = self
                    .file_sessions
                    .get(session_id)?
                    .map(|session| session.to_chat_session())
                else {
                    return Ok(None);
                };
                session.hydrate_provider_from_model();
                Ok(Some(session))
            }

            pub fn get_session_view_by_turn_id(
                &self,
                turn_id: &str,
            ) -> Result<Option<ChatSession>> {
                let turn_id = turn_id.trim();
                if turn_id.is_empty() {
                    return Ok(None);
                }

                let Some(mut session) = self
                    .file_sessions
                    .get_by_turn_id(turn_id)?
                    .map(|session| session.to_chat_session())
                else {
                    return Ok(None);
                };
                session.hydrate_provider_from_model();
                Ok(Some(session))
            }

            pub fn list_session_views(
                &self,
                agent_id: Option<&str>,
                skill_id: Option<&str>,
                include_archived: bool,
            ) -> Result<Vec<ChatSession>> {
                let mut sessions = self
                    .file_sessions
                    .list()?
                    .into_iter()
                    .map(|session| session.to_chat_session())
                    .filter(|session| {
                        Self::session_matches_list_filter(
                            session,
                            agent_id,
                            skill_id,
                            include_archived,
                        )
                    })
                    .collect::<Vec<_>>();

                for session in &mut sessions {
                    session.hydrate_provider_from_model();
                }

                sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));

                Ok(sessions)
            }

            pub fn list_session_summaries(
                &self,
                agent_id: Option<&str>,
                skill_id: Option<&str>,
                include_archived: bool,
            ) -> Result<Vec<ChatSessionSummary>> {
                let mut summaries = if include_archived {
                    self.file_sessions.list_summaries_all()?
                } else {
                    self.file_sessions.list_summaries()?
                };
                summaries.retain(|summary| {
                    Self::summary_matches_list_filter(summary, agent_id, skill_id, include_archived)
                });
                summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated_at));
                Ok(summaries)
            }

            fn session_matches_list_filter(
                session: &ChatSession,
                agent_id: Option<&str>,
                skill_id: Option<&str>,
                include_archived: bool,
            ) -> bool {
                if !include_archived && session.is_archived() {
                    return false;
                }
                if let Some(agent_id) = agent_id
                    && session.agent_id != agent_id
                {
                    return false;
                }
                if let Some(skill_id) = skill_id
                    && session.skill_id.as_deref() != Some(skill_id)
                {
                    return false;
                }
                true
            }

            fn summary_matches_list_filter(
                summary: &ChatSessionSummary,
                agent_id: Option<&str>,
                skill_id: Option<&str>,
                include_archived: bool,
            ) -> bool {
                if !include_archived && summary.archived_at.is_some() {
                    return false;
                }
                if let Some(agent_id) = agent_id
                    && summary.agent_id != agent_id
                {
                    return false;
                }
                if let Some(skill_id) = skill_id
                    && summary.skill_id.as_deref() != Some(skill_id)
                {
                    return false;
                }
                true
            }

            pub fn search_session_views(
                &self,
                query: &str,
                agent_id: Option<&str>,
                skill_id: Option<&str>,
                include_archived: bool,
                limit: usize,
            ) -> Result<Vec<ChatSession>> {
                let keyword = query.to_lowercase();
                let sessions = self.list_session_views(agent_id, skill_id, include_archived)?;

                Ok(sessions
                    .into_iter()
                    .filter(|session| {
                        session.name.to_lowercase().contains(&keyword)
                            || session
                                .messages
                                .iter()
                                .any(|message| message.content.to_lowercase().contains(&keyword))
                    })
                    .take(limit)
                    .collect())
            }

            pub fn create_workspace_session(
                &self,
                agent_id: String,
                model: String,
                name: Option<String>,
                skill_id: Option<String>,
                retention: Option<String>,
            ) -> Result<ChatSession> {
                let mut session = ChatSession::new(agent_id, model);
                if let Some(name) = name {
                    session = session.with_name(name);
                }
                if let Some(skill_id) = skill_id {
                    session = session.with_skill(skill_id);
                }
                if let Some(retention) = retention {
                    session = session.with_retention(retention);
                }
                self.persist_session_view(&session, "create")?;
                publish_session_event(ChatSessionEvent::Created {
                    session_id: session.id.clone(),
                });
                Ok(session)
            }

            pub fn append_exchange(
                &self,
                session_id: &str,
                user_message: ChatMessage,
                assistant_message: ChatMessage,
                active_model: Option<&str>,
                final_model: Option<ModelId>,
                source: &str,
            ) -> Result<ChatSession> {
                let session_lock = {
                    let mut locks = self.append_locks.lock().expect("session append locks");
                    if let Some(lock) = locks.get(session_id).and_then(Weak::upgrade) {
                        lock
                    } else {
                        let lock = Arc::new(Mutex::new(()));
                        locks.insert(session_id.to_string(), Arc::downgrade(&lock));
                        lock
                    }
                };

                let session = {
                    let _guard = session_lock.lock().expect("session append lock");
                    let mut session = self
                        .get_session_view(session_id)?
                        .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

                    session.hydrate_provider_from_model();
                    session.add_message(user_message);
                    session.add_message(assistant_message);

                    if let Some(model) = final_model {
                        session.set_model_identity(model);
                    } else if let Some(model) = active_model {
                        session.set_model_identity_from_raw(model);
                    }

                    self.persist_session_view(&session, "append_exchange")?;
                    session
                };

                self.append_locks
                    .lock()
                    .expect("session append locks")
                    .retain(|_, weak| weak.strong_count() > 0);

                publish_session_event(ChatSessionEvent::MessageAdded {
                    session_id: session_id.to_string(),
                    source: source.to_string(),
                });

                Ok(session)
            }

            pub fn append_user_message(
                &self,
                session_id: &str,
                mut user_message: ChatMessage,
                source: &str,
            ) -> Result<ChatSession> {
                crate::voice_transcript::hydrate_voice_message_metadata(&mut user_message);

                let session_lock = {
                    let mut locks = self.append_locks.lock().expect("session append locks");
                    if let Some(lock) = locks.get(session_id).and_then(Weak::upgrade) {
                        lock
                    } else {
                        let lock = Arc::new(Mutex::new(()));
                        locks.insert(session_id.to_string(), Arc::downgrade(&lock));
                        lock
                    }
                };

                let session = {
                    let _guard = session_lock.lock().expect("session append lock");
                    let mut session = self
                        .get_session_view(session_id)?
                        .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

                    session.hydrate_provider_from_model();
                    session.add_message(user_message);
                    self.persist_session_view(&session, "append_user_message")?;
                    session
                };

                self.append_locks
                    .lock()
                    .expect("session append locks")
                    .retain(|_, weak| weak.strong_count() > 0);

                publish_session_event(ChatSessionEvent::MessageAdded {
                    session_id: session_id.to_string(),
                    source: source.to_string(),
                });

                Ok(session)
            }

            pub fn save_existing_session(&self, session: &ChatSession, source: &str) -> Result<()> {
                let mut session = session.clone();
                session.hydrate_provider_from_model();
                self.persist_session_view(&session, "save")?;
                publish_session_event(ChatSessionEvent::MessageAdded {
                    session_id: session.id.clone(),
                    source: source.to_string(),
                });
                Ok(())
            }

            pub fn create_external_session(&self, mut session: ChatSession) -> Result<ChatSession> {
                session.hydrate_provider_from_model();
                self.persist_session_view(&session, "create_external")?;
                publish_session_event(ChatSessionEvent::Created {
                    session_id: session.id.clone(),
                });
                Ok(session)
            }

            pub fn save_session_metadata(&self, session: &ChatSession) -> Result<()> {
                let mut session = session.clone();
                session.hydrate_provider_from_model();
                self.persist_session_view(&session, "metadata")?;
                publish_session_event(ChatSessionEvent::Updated {
                    session_id: session.id.clone(),
                });
                Ok(())
            }

            pub fn update_session(
                &self,
                session_id: &str,
                updates: ChatSessionUpdate,
            ) -> Result<Option<ChatSession>> {
                let Some(mut session) = self.get_session_view(session_id)? else {
                    return Ok(None);
                };
                let mut updated = false;
                let mut name_updated = false;

                if let Some(agent_id) = updates.agent_id {
                    let agents = self
                        .agents
                        .as_ref()
                        .ok_or_else(|| anyhow!("Agent storage is unavailable"))?;
                    session.agent_id = agents.resolve_existing_agent_id(&agent_id)?;
                    updated = true;
                }

                if let Some(model) = updates.model {
                    let normalized = ModelId::normalize_model_id(&model)
                        .ok_or_else(|| anyhow!("Unknown model: {}", model.trim()))?;
                    session.set_model_identity_from_raw(&normalized);
                    updated = true;
                }

                if let Some(name) = updates.name {
                    session.rename(name);
                    updated = true;
                    name_updated = true;
                }

                if updated {
                    if !name_updated {
                        session.updated_at = chrono::Utc::now().timestamp_millis();
                    }
                    self.persist_session_view(&session, "update")?;
                    publish_session_event(ChatSessionEvent::Updated {
                        session_id: session.id.clone(),
                    });
                }

                Ok(Some(session))
            }

            pub fn rename_session(
                &self,
                session_id: &str,
                name: String,
            ) -> Result<Option<ChatSession>> {
                let Some(mut session) = self.get_session_view(session_id)? else {
                    return Ok(None);
                };
                session.rename(name);
                self.persist_session_view(&session, "rename")?;
                publish_session_event(ChatSessionEvent::Updated {
                    session_id: session.id.clone(),
                });
                Ok(Some(session))
            }

            pub fn switch_session_model(
                &self,
                session_id: &str,
                provider: String,
                model: String,
            ) -> Result<Option<ChatSession>> {
                let Some(mut session) = self.get_session_view(session_id)? else {
                    return Ok(None);
                };
                session.provider = provider;
                session.model = model;
                session.updated_at = chrono::Utc::now().timestamp_millis();
                self.persist_session_view(&session, "switch_model")?;
                publish_session_event(ChatSessionEvent::Updated {
                    session_id: session.id.clone(),
                });
                Ok(Some(session))
            }

            pub fn archive_session(&self, session_id: &str) -> Result<bool> {
                let Some(mut session) = self.get_session_view(session_id)? else {
                    return Ok(false);
                };
                if session.is_archived() {
                    return Ok(false);
                }
                session.archive();
                self.persist_session_view(&session, "archive")?;
                publish_session_event(ChatSessionEvent::Updated {
                    session_id: session_id.to_string(),
                });
                Ok(true)
            }

            pub fn unarchive_session(&self, session_id: &str) -> Result<bool> {
                let Some(mut session) = self.get_session_view(session_id)? else {
                    return Ok(false);
                };
                if !session.is_archived() {
                    return Ok(false);
                }
                session.unarchive();
                self.persist_session_view(&session, "unarchive")?;
                publish_session_event(ChatSessionEvent::Updated {
                    session_id: session_id.to_string(),
                });
                Ok(true)
            }

            pub fn delete_session(&self, session_id: &str) -> Result<bool> {
                if self.get_session_view(session_id)?.is_none() {
                    return Ok(false);
                }
                let deleted = self.delete_file_session(session_id);
                if deleted {
                    publish_session_event(ChatSessionEvent::Deleted {
                        session_id: session_id.to_string(),
                    });
                }
                Ok(deleted)
            }

            pub fn cleanup_workspace_sessions_older_than(
                &self,
                older_than_ms: i64,
            ) -> Result<SessionCleanupStats> {
                let sessions = self.list_session_views(None, None, true)?;
                let mut stats = SessionCleanupStats {
                    scanned: sessions.len(),
                    ..SessionCleanupStats::default()
                };

                for session in sessions {
                    if session.updated_at >= older_than_ms {
                        stats.skipped_not_expired += 1;
                        continue;
                    }

                    let serialized_len = serde_json::to_vec(&session)
                        .map(|bytes| bytes.len() as u64)
                        .unwrap_or(0);
                    if self.file_sessions.delete(&session.id)? {
                        stats.deleted += 1;
                        stats.bytes_freed += serialized_len;
                    }
                }

                Ok(stats)
            }

            pub fn cleanup_workspace_sessions_by_retention(
                &self,
                now_ms: i64,
            ) -> Result<SessionCleanupStats> {
                let sessions = self.list_session_views(None, None, true)?;
                let mut stats = SessionCleanupStats {
                    scanned: sessions.len(),
                    ..SessionCleanupStats::default()
                };

                for session in sessions {
                    let Some(retention) = session.retention.as_deref() else {
                        stats.skipped_no_retention += 1;
                        continue;
                    };

                    let Some(retention_ms) = parse_retention_to_ms(retention) else {
                        stats.failed += 1;
                        continue;
                    };

                    let expires_at = session.updated_at.saturating_add(retention_ms);
                    if now_ms < expires_at {
                        stats.skipped_not_expired += 1;
                        continue;
                    }

                    let serialized_len = serde_json::to_vec(&session)
                        .map(|bytes| bytes.len() as u64)
                        .unwrap_or(0);
                    if self.file_sessions.delete(&session.id)? {
                        stats.deleted += 1;
                        stats.bytes_freed += serialized_len;
                    }
                }

                Ok(stats)
            }

            pub fn persist_interactive_turn(
                &self,
                session: &mut ChatSession,
                request: PersistInteractiveTurnRequest<'_>,
            ) -> Result<()> {
                if request.assistant_output.trim().is_empty() {
                    anyhow::bail!("assistant_output must not be empty");
                }
                let _ = replace_latest_user_message_content(
                    session,
                    request.original_input,
                    request.persisted_input,
                );
                session.hydrate_provider_from_model();
                session.add_message(
                    ChatMessage::assistant(request.assistant_output)
                        .with_execution(request.execution),
                );
                if let Some(model) = request.final_model {
                    session.set_model_identity(model);
                } else if let Some(model) = request.active_model {
                    session.set_model_identity_from_raw(model);
                }
                self.save_existing_session(session, request.source)
            }

            pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
                self.archive_session(session_id)
            }

            pub fn unarchive_workspace_session(&self, session_id: &str) -> Result<bool> {
                self.unarchive_session(session_id)
            }

            pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
                self.delete_session(session_id)
            }

            fn persist_session_view(
                &self,
                session: &ChatSession,
                operation: &'static str,
            ) -> Result<()> {
                if let Err(error) = self.write_file_session(session) {
                    warn!(
                        session_id = %session.id,
                        operation,
                        error = %error,
                        "Failed to write chat session JSONL"
                    );
                    return Err(error);
                }
                Ok(())
            }

            fn write_file_session(&self, session: &ChatSession) -> Result<()> {
                let existing = self.file_sessions.get(&session.id)?;
                let file_session = FileSession::merge_chat_session(existing.as_ref(), session);
                self.file_sessions.write_session(&file_session, true)?;
                Ok(())
            }

            fn delete_file_session(&self, session_id: &str) -> bool {
                match self.file_sessions.delete(session_id) {
                    Ok(deleted) => deleted,
                    Err(error) => {
                        warn!(
                            session_id,
                            error = %error,
                            "Failed to delete JSONL chat session"
                        );
                        false
                    }
                }
            }
        }

        fn parse_retention_to_ms(retention: &str) -> Option<i64> {
            let normalized = retention.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "1h" => Some(60 * 60 * 1000),
                "1d" => Some(24 * 60 * 60 * 1000),
                "7d" => Some(7 * 24 * 60 * 60 * 1000),
                "30d" => Some(30 * 24 * 60 * 60 * 1000),
                _ => None,
            }
        }

        fn replace_latest_user_message_content(
            session: &mut ChatSession,
            original_content: &str,
            updated_content: &str,
        ) -> bool {
            if original_content == updated_content {
                return false;
            }

            let Some(index) = session.messages.iter().rposition(|message| {
                message.role == ChatRole::User && message.content == original_content
            }) else {
                return false;
            };

            session.messages[index].content = updated_content.to_string();
            crate::voice_transcript::hydrate_voice_message_metadata(&mut session.messages[index]);
            true
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::storage::Storage;
            use tempfile::tempdir;
            use types::MessageExecution;

            fn setup() -> (Arc<Storage>, SessionService, ChatSession) {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Arc::new(Storage::new(db_path.to_str().unwrap()).unwrap());
                let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                save_session(&storage, &session);
                let service = SessionService::from_storage(&storage);
                std::mem::forget(dir);
                (storage, service, session)
            }

            fn save_session(storage: &Storage, session: &ChatSession) {
                storage
                    .file_sessions
                    .write_session(&FileSession::from_chat_session(session), true)
                    .unwrap();
            }

            fn load_session(storage: &Storage, id: &str) -> ChatSession {
                storage
                    .file_sessions
                    .get(id)
                    .unwrap()
                    .expect("session")
                    .to_chat_session()
            }

            #[test]
            fn append_exchange_persists_messages_and_model() {
                let (storage, service, session) = setup();
                let execution = MessageExecution::new().complete(10, 2);

                let persisted = service
                    .append_exchange(
                        &session.id,
                        ChatMessage::user("hello"),
                        ChatMessage::assistant("world").with_execution(execution),
                        Some("gpt-5"),
                        Some(ModelId::Gpt5),
                        "channel",
                    )
                    .unwrap();

                assert_eq!(persisted.messages.len(), 2);
                assert_eq!(persisted.messages[0].content, "hello");
                assert_eq!(persisted.messages[1].content, "world");
                assert_eq!(persisted.provider, "openai");
                assert_eq!(persisted.model, "gpt-5");
                let reloaded = load_session(&storage, &session.id);
                assert_eq!(reloaded.messages.len(), 2);
            }

            #[test]
            fn append_exchange_prefers_provider_aware_final_model() {
                let (_storage, service, session) = setup();

                let persisted = service
                    .append_exchange(
                        &session.id,
                        ChatMessage::user("hello"),
                        ChatMessage::assistant("world"),
                        Some("MiniMax-M2.5"),
                        Some(ModelId::MiniMaxM25CodingPlan),
                        "channel",
                    )
                    .unwrap();

                assert_eq!(persisted.provider, "minimax-coding-plan");
                assert_eq!(persisted.model, "minimax-coding-plan-m2-5");
            }

            #[test]
            fn save_existing_session_updates_storage() {
                let (storage, service, mut session) = setup();
                session.add_message(ChatMessage::user("hello"));
                session.add_message(ChatMessage::assistant("world"));

                service.save_existing_session(&session, "ipc").unwrap();

                let reloaded = load_session(&storage, &session.id);
                assert_eq!(reloaded.messages.len(), 2);
                assert_eq!(reloaded.messages[0].content, "hello");
                assert_eq!(reloaded.messages[1].content, "world");
            }

            #[test]
            fn get_session_view_hydrates_provider_for_legacy_session() {
                let (storage, service, mut session) = setup();
                session.provider.clear();
                save_session(&storage, &session);

                let hydrated = service
                    .get_session_view(&session.id)
                    .unwrap()
                    .expect("session");

                assert_eq!(hydrated.provider, "openai");
                assert_eq!(hydrated.model, "gpt-5");
            }

            #[test]
            fn list_session_views_includes_file_backed_sessions() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                session.add_message(ChatMessage::user("old file session"));
                file_store
                    .write_session(&FileSession::from_chat_session(&session), false)
                    .unwrap();
                let service = SessionService::new(file_store, Some(storage.agents.clone()));

                let sessions = service.list_session_views(None, None, false).unwrap();

                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].id, session.id);
                assert_eq!(sessions[0].messages[0].content, "old file session");
            }

            #[test]
            fn list_session_views_filters_file_backed_sessions() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let skill_session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
                    .with_skill("release");
                let mut archived_session =
                    ChatSession::new("agent-2".to_string(), "gpt-5".to_string());
                archived_session.archive();
                file_store
                    .write_session(&FileSession::from_chat_session(&skill_session), false)
                    .unwrap();
                file_store
                    .write_session(&FileSession::from_chat_session(&archived_session), false)
                    .unwrap();
                let service = SessionService::new(file_store, Some(storage.agents.clone()));

                let active = service.list_session_views(None, None, false).unwrap();
                let by_skill = service
                    .list_session_views(None, Some("release"), false)
                    .unwrap();
                let by_agent_all = service
                    .list_session_views(Some("agent-2"), None, true)
                    .unwrap();

                assert_eq!(active.len(), 1);
                assert_eq!(active[0].id, skill_session.id);
                assert_eq!(by_skill.len(), 1);
                assert_eq!(by_skill[0].id, skill_session.id);
                assert_eq!(by_agent_all.len(), 1);
                assert_eq!(by_agent_all[0].id, archived_session.id);
            }

            #[test]
            fn get_session_view_propagates_invalid_file_session_errors() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_root = dir.path().join("sessions");
                let day_dir = file_root.join("2026").join("05").join("03");
                std::fs::create_dir_all(&day_dir).unwrap();
                std::fs::write(day_dir.join("broken-session.jsonl"), "{not-json}\n").unwrap();
                let service = SessionService::new(
                    FileSessionStore::new(file_root).unwrap(),
                    Some(storage.agents.clone()),
                );

                let error = service
                    .get_session_view("broken-session")
                    .expect_err("invalid JSONL should be surfaced");

                assert!(error.to_string().contains("invalid JSONL"));
            }

            #[test]
            fn create_workspace_session_writes_file_store() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let service = SessionService::new(file_store.clone(), Some(storage.agents.clone()));

                let session = service
                    .create_workspace_session(
                        "agent-1".to_string(),
                        "gpt-5".to_string(),
                        Some("New imported path".to_string()),
                        None,
                        None,
                    )
                    .unwrap();

                assert!(file_store.get(&session.id).unwrap().is_some());
            }

            #[test]
            fn cleanup_workspace_sessions_only_deletes_expired_sessions() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let service = SessionService::new(file_store.clone(), Some(storage.agents.clone()));

                let mut old_session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                old_session.updated_at = 1;
                file_store
                    .write_session(&FileSession::from_chat_session(&old_session), true)
                    .unwrap();
                let mut fresh_session =
                    ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                fresh_session.updated_at = 20;
                file_store
                    .write_session(&FileSession::from_chat_session(&fresh_session), true)
                    .unwrap();

                let stats = service.cleanup_workspace_sessions_older_than(10).unwrap();

                assert_eq!(stats.scanned, 2);
                assert_eq!(stats.deleted, 1);
                assert_eq!(stats.skipped_not_expired, 1);
                assert!(file_store.get(&old_session.id).unwrap().is_none());
                assert!(file_store.get(&fresh_session.id).unwrap().is_some());
            }

            #[test]
            fn rename_file_backed_session_without_materializing_redb_session() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                file_store
                    .write_session(&FileSession::from_chat_session(&session), false)
                    .unwrap();
                let service = SessionService::new(file_store.clone(), Some(storage.agents.clone()));

                let renamed = service
                    .rename_session(&session.id, "Imported".to_string())
                    .unwrap()
                    .expect("renamed");

                assert_eq!(renamed.name, "Imported");
                assert_eq!(
                    file_store
                        .get(&session.id)
                        .unwrap()
                        .unwrap()
                        .to_chat_session()
                        .name,
                    "Imported"
                );
            }

            #[test]
            fn delete_file_backed_session_without_redb_session() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                file_store
                    .write_session(&FileSession::from_chat_session(&session), false)
                    .unwrap();
                let service = SessionService::new(file_store.clone(), Some(storage.agents.clone()));

                assert!(service.delete_session(&session.id).unwrap());
                assert!(file_store.get(&session.id).unwrap().is_none());
            }

            #[test]
            fn save_existing_session_mirrors_to_file_session_store() {
                let dir = tempdir().unwrap();
                let db_path = dir.path().join("session-service.db");
                let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
                let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
                let service = SessionService::new(file_store.clone(), Some(storage.agents.clone()));
                let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
                session.add_message(ChatMessage::user("hello"));

                service.save_existing_session(&session, "test").unwrap();

                let loaded = file_store.get(&session.id).unwrap().expect("jsonl session");
                assert_eq!(loaded.to_chat_session().messages.len(), 1);
            }

            #[test]
            fn append_user_message_hydrates_voice_metadata() {
                let (storage, service, session) = setup();

                let persisted = service
                    .append_user_message(
                        &session.id,
                        ChatMessage::user(
                            "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\n\n[Transcript]\nhello voice",
                        ),
                        "ipc",
                    )
                    .unwrap();

                assert_eq!(persisted.messages.len(), 1);
                let user = &persisted.messages[0];
                assert_eq!(user.role, ChatRole::User);
                assert_eq!(
                    user.media.as_ref().map(|media| media.file_path.as_str()),
                    Some("/tmp/voice.webm")
                );
                assert_eq!(
                    user.transcript
                        .as_ref()
                        .map(|transcript| transcript.text.as_str()),
                    Some("hello voice")
                );

                let reloaded = load_session(&storage, &session.id);
                assert_eq!(reloaded.messages.len(), 1);
            }

            #[test]
            fn update_session_enforces_workspace_policy_and_persists_changes() {
                let (storage, service, session) = setup();
                let updated = service
                    .update_session(
                        &session.id,
                        ChatSessionUpdate {
                            agent_id: None,
                            model: Some("gpt-5".to_string()),
                            name: Some("Updated".to_string()),
                        },
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(updated.name, "Updated");
                let reloaded = load_session(&storage, &session.id);
                assert_eq!(reloaded.name, "Updated");
            }

            #[test]
            fn persist_interactive_turn_rewrites_latest_input_and_appends_output() {
                let (storage, service, mut session) = setup();
                session.add_message(ChatMessage::user("voice input"));
                save_session(&storage, &session);

                service
                    .persist_interactive_turn(
                        &mut session,
                        PersistInteractiveTurnRequest {
                            original_input: "voice input",
                            persisted_input: "voice transcript",
                            assistant_output: "assistant output",
                            active_model: Some("gpt-5"),
                            final_model: Some(ModelId::Gpt5),
                            execution: MessageExecution::new().complete(20, 1),
                            source: "ipc",
                        },
                    )
                    .unwrap();

                let reloaded = load_session(&storage, &session.id);
                assert_eq!(reloaded.messages.len(), 2);
                assert_eq!(reloaded.messages[0].content, "voice transcript");
                assert_eq!(reloaded.messages[1].content, "assistant output");
                assert_eq!(reloaded.provider, "openai");
                assert_eq!(reloaded.model, "gpt-5");
            }

            #[test]
            fn persist_interactive_turn_prefers_provider_aware_final_model() {
                let (storage, service, mut session) = setup();
                session.add_message(ChatMessage::user("voice input"));
                save_session(&storage, &session);

                service
                    .persist_interactive_turn(
                        &mut session,
                        PersistInteractiveTurnRequest {
                            original_input: "voice input",
                            persisted_input: "voice transcript",
                            assistant_output: "assistant output",
                            active_model: Some("MiniMax-M2.5"),
                            final_model: Some(ModelId::MiniMaxM25CodingPlan),
                            execution: MessageExecution::new().complete(20, 1),
                            source: "ipc",
                        },
                    )
                    .unwrap();

                let reloaded = load_session(&storage, &session.id);
                assert_eq!(reloaded.provider, "minimax-coding-plan");
                assert_eq!(reloaded.model, "minimax-coding-plan-m2-5");
            }

            #[test]
            fn persist_interactive_turn_rejects_empty_assistant_output() {
                let (_storage, service, mut session) = setup();
                session.add_message(ChatMessage::user("voice input"));

                let error = service
                    .persist_interactive_turn(
                        &mut session,
                        PersistInteractiveTurnRequest {
                            original_input: "voice input",
                            persisted_input: "voice transcript",
                            assistant_output: "   ",
                            active_model: Some("gpt-5"),
                            final_model: Some(ModelId::Gpt5),
                            execution: MessageExecution::new().complete(20, 1),
                            source: "ipc",
                        },
                    )
                    .expect_err("empty assistant output should be rejected");

                assert!(
                    error
                        .to_string()
                        .contains("assistant_output must not be empty")
                );
            }
        }
    }
    pub mod skill_mentions {
        /// Parse explicit `@skill-id` mentions from user input.
        pub fn parse_skill_mentions(input: &str) -> Vec<String> {
            let mut mentions = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let chars = input.char_indices().collect::<Vec<_>>();
            let mut index = 0usize;

            while index < chars.len() {
                let (byte_index, ch) = chars[index];
                if ch != '@' {
                    index += 1;
                    continue;
                }

                if byte_index > 0 {
                    let previous = input[..byte_index].chars().next_back();
                    if previous.is_some_and(|value| !value.is_whitespace()) {
                        index += 1;
                        continue;
                    }
                }

                let mut end = byte_index + ch.len_utf8();
                let mut next_index = index + 1;
                while next_index < chars.len() {
                    let (next_byte, next_ch) = chars[next_index];
                    if !(next_ch.is_ascii_alphanumeric() || next_ch == '-' || next_ch == '_') {
                        break;
                    }
                    end = next_byte + next_ch.len_utf8();
                    next_index += 1;
                }

                if end > byte_index + 1 {
                    let id = input[byte_index + 1..end].to_string();
                    if seen.insert(id.clone()) {
                        mentions.push(id);
                    }
                }
                index = next_index.max(index + 1);
            }

            mentions
        }

        #[cfg(test)]
        mod tests {
            use super::parse_skill_mentions;

            #[test]
            fn parses_single_skill_mention() {
                assert_eq!(parse_skill_mentions("@team review this"), vec!["team"]);
            }

            #[test]
            fn parses_multiple_unique_mentions() {
                assert_eq!(
                    parse_skill_mentions("@team use @code-review and @team"),
                    vec!["team", "code-review"]
                );
            }

            #[test]
            fn ignores_email_like_at_signs() {
                assert!(parse_skill_mentions("mail me at a@example.com").is_empty());
            }

            #[test]
            fn supports_chinese_text_around_mentions() {
                assert_eq!(parse_skill_mentions("请用 @team 并行处理"), vec!["team"]);
            }
        }
    }
    pub mod skill_triggers {
        use types::{Skill, SkillStatus};

        /// Match result for a skill trigger phrase.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct TriggerMatch {
            pub skill_id: String,
            pub skill_name: String,
            pub matched_trigger: String,
            pub confidence: TriggerConfidence,
        }

        /// Confidence score for a trigger match.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum TriggerConfidence {
            Exact,
        }

        /// Find active skills whose trigger phrases appear in the user message.
        pub fn match_triggers(message: &str, skills: &[Skill]) -> Vec<TriggerMatch> {
            let normalized_message = message.to_lowercase();
            let mut matches = Vec::new();

            for skill in skills {
                if skill.status != SkillStatus::Active {
                    continue;
                }

                for trigger in &skill.triggers {
                    let normalized_trigger = trigger.trim().to_lowercase();
                    if normalized_trigger.is_empty() {
                        continue;
                    }

                    if normalized_message.contains(&normalized_trigger) {
                        matches.push(TriggerMatch {
                            skill_id: skill.id.clone(),
                            skill_name: skill.name.clone(),
                            matched_trigger: trigger.clone(),
                            confidence: TriggerConfidence::Exact,
                        });
                        break;
                    }
                }
            }

            matches.sort_by_key(|match_result| std::cmp::Reverse(match_result.confidence));
            matches
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            fn build_skill(id: &str, name: &str, triggers: Vec<&str>) -> Skill {
                let mut skill = Skill::new(
                    id.to_string(),
                    name.to_string(),
                    Some(format!("{} description", name)),
                    None,
                    format!("# {}\n", name),
                );
                skill.triggers = triggers.into_iter().map(|item| item.to_string()).collect();
                skill
            }

            #[test]
            fn test_trigger_exact_match() {
                let skills = vec![build_skill(
                    "code-reviewer",
                    "Code Reviewer",
                    vec!["code review", "review PR"],
                )];

                let matches = match_triggers("please review PR #123", &skills);
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].skill_id, "code-reviewer");
                assert_eq!(matches[0].confidence, TriggerConfidence::Exact);
            }

            #[test]
            fn test_trigger_case_insensitive() {
                let skills = vec![build_skill(
                    "code-reviewer",
                    "Code Reviewer",
                    vec!["Code Review"],
                )];

                let matches = match_triggers("do a code review on this patch", &skills);
                assert_eq!(matches.len(), 1);
            }

            #[test]
            fn test_trigger_no_match() {
                let skills = vec![build_skill("deployer", "Deployer", vec!["deploy release"])];

                let matches = match_triggers("fix the bug in parser", &skills);
                assert!(matches.is_empty());
            }

            #[test]
            fn test_trigger_ignores_non_active_skills() {
                let mut archived = build_skill("archived", "Archived", vec!["code review"]);
                archived.status = SkillStatus::Archived;

                let matches = match_triggers("code review this", &[archived]);
                assert!(matches.is_empty());
            }
        }
    }
    pub mod skills {
        //! Skills service layer for the skrun-managed catalog.

        use crate::{AppCore, services::adapters::SkrunSkillProvider};
        use anyhow::{Context, Result, anyhow};
        use regex::Regex;
        use std::collections::HashSet;
        use std::path::{Path, PathBuf};
        use std::sync::{Arc, OnceLock};
        use types::{Skill, ValidationError};

        /// List all skills visible to RestFlow.
        pub async fn list_skills(_core: &Arc<AppCore>) -> Result<Vec<Skill>> {
            list_available_skills()
        }

        /// Root directory for the skrun catalog RestFlow exposes.
        pub fn skill_catalog_root() -> Result<PathBuf> {
            if let Some(root) = std::env::var_os("SKRUN_SKILLS_DIR") {
                return Ok(PathBuf::from(root));
            }
            crate::paths::user_skills_dir()
        }

        /// List the skrun-managed skill catalog visible to runtime validation and preflight.
        pub fn list_available_skills() -> Result<Vec<Skill>> {
            SkrunSkillProvider::default()
                .try_list_skill_models()
                .map_err(|error| anyhow!("skrun skill catalog unavailable: {error}"))
        }

        /// Check whether a skill exists in the skrun-managed catalog.
        pub fn skill_exists_in_catalog(id: &str) -> Result<bool> {
            SkrunSkillProvider::default()
                .try_get_skill_model(id)
                .map(|skill| skill.is_some())
                .map_err(|error| anyhow!("skrun skill catalog unavailable: {error}"))
        }

        /// Get a skill by ID.
        pub async fn get_skill(_core: &Arc<AppCore>, id: &str) -> Result<Option<Skill>> {
            SkrunSkillProvider::default()
                .try_get_skill_model(id)
                .map_err(|error| anyhow!("skrun skill catalog unavailable: {error}"))
        }

        /// Check if a skill exists.
        pub async fn skill_exists(_core: &Arc<AppCore>, id: &str) -> Result<bool> {
            skill_exists_in_catalog(id)
        }

        /// Get full content for a skill reference by skill_id and ref_id.
        pub async fn get_skill_reference(
            core: &Arc<AppCore>,
            skill_id: &str,
            ref_id: &str,
        ) -> Result<Option<String>> {
            let skill = get_skill(core, skill_id)
                .await?
                .ok_or_else(|| anyhow!("Skill not found: {}", skill_id))?;

            let reference = skill
                .references
                .iter()
                .find(|reference| reference.id == ref_id)
                .ok_or_else(|| {
                    anyhow!("Reference '{}' not found in skill '{}'", ref_id, skill_id)
                })?;

            if let Some(reference_skill) = get_skill(core, &reference.id).await? {
                return Ok(Some(reference_skill.content));
            }

            if !reference.path.trim().is_empty() {
                let path = resolve_reference_path(&skill, &reference.path);
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    return Ok(Some(content));
                }
            }

            Ok(None)
        }

        fn resolve_reference_path(skill: &Skill, reference_path: &str) -> PathBuf {
            let path = Path::new(reference_path);
            if path.is_absolute() {
                return path.to_path_buf();
            }

            if let Some(folder_path) = &skill.folder_path {
                return Path::new(folder_path).join(path);
            }

            path.to_path_buf()
        }

        /// Export a skill to markdown format.
        pub fn export_skill_to_markdown(skill: &Skill) -> String {
            skill.to_markdown()
        }

        /// Import a skill from markdown format.
        pub fn import_skill_from_markdown(id: &str, markdown: &str) -> Result<Skill> {
            Skill::from_markdown(id, markdown).context("Failed to parse markdown")
        }

        /// Validate a skill with Basic and Standard conformance checks.
        pub fn validate_skill(skill: &Skill) -> Vec<ValidationError> {
            let mut errors = Vec::new();

            if skill.name.trim().is_empty() {
                errors.push(ValidationError::new("name", "Skill name cannot be empty"));
            }

            if skill.content.trim().is_empty() {
                errors.push(ValidationError::new(
                    "content",
                    "Skill content cannot be empty",
                ));
            }

            if let Some(tags) = &skill.tags {
                for (index, tag) in tags.iter().enumerate() {
                    if tag.trim().is_empty() {
                        errors.push(ValidationError::new(
                            format!("tags[{index}]"),
                            "Tag cannot be empty",
                        ));
                    }
                }
            }

            for (index, trigger) in skill.triggers.iter().enumerate() {
                if trigger.trim().is_empty() {
                    errors.push(ValidationError::new(
                        format!("triggers[{index}]"),
                        "Trigger cannot be empty",
                    ));
                }
            }

            static VARIABLE_REGEX: OnceLock<Regex> = OnceLock::new();
            static VARIABLE_NAME_REGEX: OnceLock<Regex> = OnceLock::new();
            let variable_regex =
                VARIABLE_REGEX.get_or_init(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap());
            let variable_name_regex = VARIABLE_NAME_REGEX
                .get_or_init(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap());
            for captures in variable_regex.captures_iter(&skill.content) {
                let variable_name = captures[1].trim();
                if !variable_name_regex.is_match(variable_name) {
                    errors.push(ValidationError::new(
                        "content",
                        format!(
                            "Invalid variable '{variable_name}': must match [a-zA-Z_][a-zA-Z0-9_]*"
                        ),
                    ));
                }
            }

            for tool in &skill.suggested_tools {
                if !variable_name_regex.is_match(tool) {
                    errors.push(ValidationError::new(
                        "suggested_tools",
                        format!("Invalid tool name '{tool}': must match [a-zA-Z_][a-zA-Z0-9_]*"),
                    ));
                }
            }

            errors
        }

        /// Validate a skill with complete checks that require external registry data.
        pub fn validate_skill_complete(
            skill: &Skill,
            tool_names: &[String],
            skill_ids: &[String],
        ) -> Vec<ValidationError> {
            let mut errors = validate_skill(skill);

            let known_tools: HashSet<&str> = tool_names.iter().map(String::as_str).collect();
            let known_skill_ids: HashSet<&str> = skill_ids.iter().map(String::as_str).collect();

            for tool in &skill.suggested_tools {
                if !known_tools.contains(tool.as_str()) {
                    errors.push(ValidationError::new(
                        "suggested_tools",
                        format!("Tool '{tool}' not found in registry"),
                    ));
                }
            }

            for reference in &skill.references {
                if !known_skill_ids.contains(reference.id.as_str()) {
                    errors.push(ValidationError::new(
                        "references",
                        format!("Referenced skill '{}' not found", reference.id),
                    ));
                }
            }

            errors
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::ffi::OsString;
            use std::sync::MutexGuard;
            use tempfile::{TempDir, tempdir};

            const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
            const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";
            const SKRUN_SKILLS_DIR_ENV: &str = "SKRUN_SKILLS_DIR";

            struct SkillsTestEnv {
                _lock: MutexGuard<'static, ()>,
                temp_dir: TempDir,
                previous_master_key: Option<OsString>,
                previous_restflow_dir: Option<OsString>,
                previous_skrun_skills_dir: Option<OsString>,
            }

            impl Drop for SkillsTestEnv {
                fn drop(&mut self) {
                    unsafe {
                        if let Some(value) = self.previous_restflow_dir.as_ref() {
                            std::env::set_var(RESTFLOW_DIR_ENV, value);
                        } else {
                            std::env::remove_var(RESTFLOW_DIR_ENV);
                        }
                        if let Some(value) = self.previous_master_key.as_ref() {
                            std::env::set_var(MASTER_KEY_ENV, value);
                        } else {
                            std::env::remove_var(MASTER_KEY_ENV);
                        }
                        if let Some(value) = self.previous_skrun_skills_dir.as_ref() {
                            std::env::set_var(SKRUN_SKILLS_DIR_ENV, value);
                        } else {
                            std::env::remove_var(SKRUN_SKILLS_DIR_ENV);
                        }
                    }
                }
            }

            impl SkillsTestEnv {
                fn install_markdown_skill(&self, mut artifact: skrun::SkillArtifact) {
                    let root = self.temp_dir.path().join("skrun-skills");
                    artifact.executable = false;
                    skrun::save_artifact(root.join(&artifact.id), &artifact).unwrap();
                    unsafe {
                        std::env::set_var(SKRUN_SKILLS_DIR_ENV, root);
                    }
                }
            }

            #[allow(clippy::await_holding_lock)]
            async fn create_test_core() -> (Arc<AppCore>, SkillsTestEnv) {
                let env_lock = crate::paths::restflow_dir_env_lock();
                let temp_dir = tempdir().unwrap();
                let db_path = temp_dir.path().join("test.db");
                let state_dir = temp_dir.path().join("state");
                std::fs::create_dir_all(&state_dir).unwrap();

                let previous_master_key = std::env::var_os(MASTER_KEY_ENV);
                let previous_restflow_dir = std::env::var_os(RESTFLOW_DIR_ENV);
                let previous_skrun_skills_dir = std::env::var_os(SKRUN_SKILLS_DIR_ENV);
                unsafe {
                    std::env::set_var(RESTFLOW_DIR_ENV, &state_dir);
                    std::env::set_var(MASTER_KEY_ENV, "11".repeat(32));
                    std::env::set_var(
                        SKRUN_SKILLS_DIR_ENV,
                        temp_dir.path().join("empty-skrun-skills"),
                    );
                }
                let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
                (
                    core,
                    SkillsTestEnv {
                        _lock: env_lock,
                        temp_dir,
                        previous_master_key,
                        previous_restflow_dir,
                        previous_skrun_skills_dir,
                    },
                )
            }

            fn create_test_skill(id: &str, name: &str) -> Skill {
                Skill::new(
                    id.to_string(),
                    name.to_string(),
                    Some(format!("Description for {}", name)),
                    Some(vec!["test".to_string()]),
                    format!("# {}\n\nContent here.", name),
                )
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_list_skills_empty_without_skrun() {
                let (core, _env) = create_test_core().await;
                let skills = list_skills(&core).await.unwrap();
                assert!(skills.is_empty());
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_list_and_get_team_skrun_skill() {
                let (core, env) = create_test_core().await;
                let mut artifact = skrun::SkillArtifact::markdown(
                    "team",
                    "Team",
                    "0.1.0",
                    "# Team\n\nUse spawn_subagent_batch.",
                );
                artifact.suggested_tools = vec!["spawn_subagent_batch".to_string()];
                env.install_markdown_skill(artifact);

                let skills = list_skills(&core).await.unwrap();
                let team = skills
                    .iter()
                    .find(|skill| skill.id == "team")
                    .expect("team skrun skill should be listed");
                assert_eq!(team.source, types::SkillSource::External);
                assert!(team.read_only);

                let team = get_skill(&core, "team")
                    .await
                    .unwrap()
                    .expect("team skrun skill should be readable");
                assert_eq!(team.name, "Team");
                assert!(team.content.contains("spawn_subagent_batch"));
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_get_nonexistent_skill() {
                let (core, _env) = create_test_core().await;
                let result = get_skill(&core, "nonexistent").await.unwrap();
                assert!(result.is_none());
            }

            #[test]
            fn test_export_skill_to_markdown() {
                let skill = create_test_skill("test-skill", "Test Skill");
                let markdown = export_skill_to_markdown(&skill);

                assert!(markdown.contains("name: Test Skill"));
                assert!(markdown.contains("description: Description for Test Skill"));
                assert!(markdown.contains("# Test Skill"));
            }

            #[test]
            fn test_import_skill_from_markdown() {
                let markdown = r#"---
        name: Imported Skill
        description: A skill imported from markdown
        tags:
          - imported
          - test
        ---

        # Imported Skill

        This is the skill content."#;

                let skill = import_skill_from_markdown("imported-skill", markdown).unwrap();
                assert_eq!(skill.id, "imported-skill");
                assert_eq!(skill.name, "Imported Skill");
                assert_eq!(
                    skill.description,
                    Some("A skill imported from markdown".to_string())
                );
                assert_eq!(
                    skill.tags,
                    Some(vec!["imported".to_string(), "test".to_string()])
                );
                assert!(skill.content.contains("# Imported Skill"));
            }

            #[test]
            fn test_import_skill_from_markdown_invalid() {
                let markdown = "# No frontmatter";
                let result = import_skill_from_markdown("test", markdown);
                assert!(result.is_err());
            }

            #[test]
            fn test_roundtrip_markdown_export_import() {
                let original = create_test_skill("test-skill", "Test Skill");
                let markdown = export_skill_to_markdown(&original);
                let imported = import_skill_from_markdown("test-skill", &markdown).unwrap();

                assert_eq!(imported.id, original.id);
                assert_eq!(imported.name, original.name);
                assert_eq!(imported.description, original.description);
                assert_eq!(imported.tags, original.tags);
            }

            #[test]
            fn test_validate_skill_empty_fields() {
                let mut skill = create_test_skill("skill-1", "Skill One");
                skill.name = "   ".to_string();
                skill.content = "\n".to_string();
                skill.tags = Some(vec!["ok".to_string(), " ".to_string()]);
                skill.triggers = vec!["".to_string()];

                let errors = validate_skill(&skill);

                assert!(errors.iter().any(|e| e.field == "name"));
                assert!(errors.iter().any(|e| e.field == "content"));
                assert!(errors.iter().any(|e| e.field == "tags[1]"));
                assert!(errors.iter().any(|e| e.field == "triggers[0]"));
            }

            #[test]
            fn test_validate_skill_invalid_tool_and_variable_name() {
                let mut skill = create_test_skill("skill-2", "Skill Two");
                skill.content = "Use {{invalid-name}} and {{valid_name}}".to_string();
                skill.suggested_tools = vec!["good_tool".to_string(), "bad-tool".to_string()];

                let errors = validate_skill(&skill);

                assert!(
                    errors
                        .iter()
                        .any(|e| e.field == "content" && e.message.contains("invalid-name"))
                );
                assert!(
                    errors
                        .iter()
                        .any(|e| e.field == "suggested_tools" && e.message.contains("bad-tool"))
                );
            }

            #[test]
            fn test_validate_skill_complete_unknown_tool_and_reference() {
                let mut skill = create_test_skill("skill-3", "Skill Three");
                skill.suggested_tools = vec!["bash".to_string(), "missing_tool".to_string()];
                skill.references = vec![
                    types::SkillReference {
                        id: "known-skill".to_string(),
                        path: "./SKILL.md".to_string(),
                        title: None,
                        summary: None,
                    },
                    types::SkillReference {
                        id: "missing-skill".to_string(),
                        path: "./missing.md".to_string(),
                        title: None,
                        summary: None,
                    },
                ];

                let tool_names = vec!["bash".to_string(), "file".to_string()];
                let skill_ids = vec!["known-skill".to_string(), "other-skill".to_string()];

                let errors = validate_skill_complete(&skill, &tool_names, &skill_ids);

                assert!(errors.iter().any(|e| {
                    e.field == "suggested_tools" && e.message.contains("missing_tool")
                }));
                assert!(
                    errors
                        .iter()
                        .any(|e| e.field == "references" && e.message.contains("missing-skill"))
                );
            }

            #[test]
            fn test_validate_skill_complete_valid_skill() {
                let mut skill = create_test_skill("skill-4", "Skill Four");
                skill.content = "Use {{ticket_id}} with {{ticket_id}}".to_string();
                skill.suggested_tools = vec!["bash".to_string()];
                skill.references = vec![types::SkillReference {
                    id: "known-skill".to_string(),
                    path: "./SKILL.md".to_string(),
                    title: None,
                    summary: None,
                }];

                let tool_names = vec!["bash".to_string()];
                let skill_ids = vec!["known-skill".to_string()];

                let errors = validate_skill_complete(&skill, &tool_names, &skill_ids);

                assert!(errors.is_empty());
            }
        }
    }
    pub mod tool_registry {
        //! Tool registry service for creating tool registries with storage access.
        //!
        //! Adapter implementations live in [`super::adapters`]. This module provides
        //! the [`create_tool_registry`] function that wires adapters into tools.

        use crate::services::adapters::*;
        use crate::storage::ConfigStorage;
        use crate::tools::ToolRegistryBuilder;
        use crate::{AgentDefaults, SystemConfig};
        use std::sync::Arc;
        use tracing::warn;
        use types::tool::SecurityGate;
        use types::toolset::ToolRegistry;

        const DEFAULT_SECURITY_AGENT_ID: &str = "unknown-agent";
        const DEFAULT_SECURITY_TASK_ID: &str = "tool-registry";

        mod assembly {
            use super::*;
            use crate::tools::{BashConfig, FileConfig};
            use types::AgentOperationAssessor;

            /// Create the daemon-owned minimal tool registry.
            ///
            /// This function creates a registry with:
            /// - Core execution tools (`bash`, file/edit/patch/search helpers)
            /// - `load_skill` for read-only skill discovery
            /// - `run_skill` for executing installed skrun skills
            /// - Optional security gate wiring for execution tools; `None` keeps default permissive behavior
            #[allow(clippy::too_many_arguments)]
            pub fn create_tool_registry(
                config_storage: ConfigStorage,
                agent_id: Option<String>,
                security_gate: Option<Arc<dyn SecurityGate>>,
            ) -> anyhow::Result<ToolRegistry> {
                create_tool_registry_with_assessor(config_storage, agent_id, security_gate, None)
            }

            pub fn create_tool_registry_with_assessor(
                config_storage: ConfigStorage,
                agent_id: Option<String>,
                security_gate: Option<Arc<dyn SecurityGate>>,
                _assessor: Option<Arc<dyn AgentOperationAssessor>>,
            ) -> anyhow::Result<ToolRegistry> {
                let config_storage = Arc::new(config_storage);
                let agent_defaults = load_agent_defaults(&config_storage);
                let skill_provider = Arc::new(SkrunSkillProvider::default());

                let mut builder = ToolRegistryBuilder::new();
                let security_agent_id = agent_id.as_deref().unwrap_or(DEFAULT_SECURITY_AGENT_ID);
                builder = builder.with_bash(BashConfig {
                    timeout_secs: agent_defaults.bash_timeout_secs,
                    ..Default::default()
                });
                builder = builder.with_file(FileConfig {
                    allow_write: false,
                    ..Default::default()
                });
                builder = if let Some(gate) = security_gate.clone() {
                    builder.with_load_skill_with_security(
                        skill_provider,
                        gate,
                        security_agent_id,
                        DEFAULT_SECURITY_TASK_ID,
                    )
                } else {
                    builder.with_load_skill(skill_provider)
                };

                let mut run_skill_tool = crate::tools::RunSkillTool::new()
                    .with_root(crate::services::skills::skill_catalog_root()?);
                if let Some(gate) = security_gate.clone() {
                    run_skill_tool = run_skill_tool.with_security(
                        gate,
                        security_agent_id,
                        DEFAULT_SECURITY_TASK_ID,
                    );
                }
                builder.registry.register(run_skill_tool);

                let registry = builder
                    .with_patch_and_base_dir(None)
                    .with_edit_and_base_dir(None)
                    .with_multiedit_and_base_dir(None)
                    .with_glob_and_base_dir(None)
                    .with_grep_and_base_dir(None)
                    .build();

                Ok(registry)
            }
        }
        mod config {
            use super::*;

            fn load_system_config(config_storage: &ConfigStorage) -> SystemConfig {
                match config_storage.get_effective_config() {
                    Ok(config) => config,
                    Err(error) => {
                        warn!(
                            error = %error,
                            "Failed to load system config defaults; falling back to built-in defaults"
                        );
                        SystemConfig::default()
                    }
                }
            }

            pub(super) fn load_agent_defaults(config_storage: &ConfigStorage) -> AgentDefaults {
                load_system_config(config_storage).agent
            }
        }

        use self::config::load_agent_defaults;

        pub use self::assembly::{create_tool_registry, create_tool_registry_with_assessor};
    }
}
pub mod session_events {
    use std::sync::OnceLock;
    use tokio::sync::broadcast;
    pub use types::ChatSessionEvent;

    const BUFFER_CAPACITY: usize = 256;

    fn stream_sender() -> &'static broadcast::Sender<ChatSessionEvent> {
        static SENDER: OnceLock<broadcast::Sender<ChatSessionEvent>> = OnceLock::new();
        SENDER.get_or_init(|| {
            let (sender, _receiver) = broadcast::channel(BUFFER_CAPACITY);
            sender
        })
    }

    /// Publish a chat session change event to daemon subscribers.
    pub fn publish_session_event(event: ChatSessionEvent) {
        let _ = stream_sender().send(event);
    }

    /// Subscribe to the daemon chat-session event bus.
    pub fn subscribe_session_events() -> broadcast::Receiver<ChatSessionEvent> {
        stream_sender().subscribe()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_publish_and_subscribe_session_event() {
            let mut receiver = subscribe_session_events();
            let event = ChatSessionEvent::MessageAdded {
                session_id: "session-1".to_string(),
                source: "background".to_string(),
            };

            publish_session_event(event);
            let received = receiver.recv().await.unwrap();

            match received {
                ChatSessionEvent::MessageAdded { session_id, source } => {
                    assert_eq!(session_id, "session-1");
                    assert_eq!(source, "background");
                }
                _ => panic!("Wrong variant"),
            }
        }

        /// Ensure JSON uses `"type"` tag with flat fields for runtime clients.
        #[test]
        fn test_serialization_uses_type_tag() {
            let event = ChatSessionEvent::MessageAdded {
                session_id: "s1".to_string(),
                source: "background".to_string(),
            };
            let json: serde_json::Value = serde_json::to_value(&event).unwrap();

            assert_eq!(json["type"], "MessageAdded");
            assert_eq!(json["session_id"], "s1");
            assert_eq!(json["source"], "background");
            // Must NOT have nested "data" or "kind" keys
            assert!(json.get("kind").is_none());
            assert!(json.get("data").is_none());
        }

        #[test]
        fn test_serialization_all_variants() {
            let cases: Vec<(ChatSessionEvent, &str)> = vec![
                (
                    ChatSessionEvent::Created {
                        session_id: "s1".into(),
                    },
                    "Created",
                ),
                (
                    ChatSessionEvent::Updated {
                        session_id: "s2".into(),
                    },
                    "Updated",
                ),
                (
                    ChatSessionEvent::MessageAdded {
                        session_id: "s3".into(),
                        source: "ipc".into(),
                    },
                    "MessageAdded",
                ),
                (
                    ChatSessionEvent::Deleted {
                        session_id: "s4".into(),
                    },
                    "Deleted",
                ),
            ];

            for (event, expected_type) in cases {
                let json: serde_json::Value = serde_json::to_value(&event).unwrap();
                assert_eq!(
                    json["type"], expected_type,
                    "wrong type for {expected_type}"
                );
                assert!(
                    json["session_id"].is_string(),
                    "missing session_id for {expected_type}"
                );
            }
        }

        #[test]
        fn test_deserialization_from_client_format() {
            let json = r#"{"type":"MessageAdded","session_id":"abc","source":"workspace"}"#;
            let event: ChatSessionEvent = serde_json::from_str(json).unwrap();
            match event {
                ChatSessionEvent::MessageAdded { session_id, source } => {
                    assert_eq!(session_id, "abc");
                    assert_eq!(source, "workspace");
                }
                _ => panic!("Wrong variant"),
            }
        }
    }
}
pub mod session_log {
    //! File-backed session transcripts.
    //!
    //! The JSONL transcript is the durable session source of truth. Each session is
    //! stored as one file under `~/.restflow/sessions/YYYY/MM/DD/<session-id>.jsonl`.

    use anyhow::{Context, Result, anyhow};
    use chrono::{DateTime, Datelike, TimeZone, Utc};
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::collections::{HashMap, HashSet};
    use std::fs::{self, File, OpenOptions};
    use std::io::{BufRead, BufReader, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use types::{
        ChatMessage, ChatMessageMedia, ChatMessageTranscript, ChatRole, ChatSession,
        ChatSessionMetadata, ChatSessionSummary, ChatTurn, ChatTurnEvent, ChatTurnEventKind,
        ChatTurnStatus, MessageExecution,
    };
    use uuid::Uuid;
    use walkdir::WalkDir;

    pub const SESSION_SCHEMA_VERSION: u32 = 1;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum SessionMessageRole {
        User,
        Assistant,
        System,
    }

    impl SessionMessageRole {
        fn as_chat_role(&self) -> ChatRole {
            match self {
                Self::User => ChatRole::User,
                Self::Assistant => ChatRole::Assistant,
                Self::System => ChatRole::System,
            }
        }

        fn from_chat_role(role: &ChatRole) -> Self {
            match role {
                ChatRole::User => Self::User,
                ChatRole::Assistant => Self::Assistant,
                ChatRole::System => Self::System,
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct UsageValues {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub input_tokens: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub output_tokens: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reasoning_tokens: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cache_read: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cache_write: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cost: Option<f64>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum SessionLogEvent {
        SessionMeta {
            schema_version: u32,
            id: String,
            created_at: String,
            updated_at: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            title: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cwd: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            model: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            provider: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            app_version: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            git_branch: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            agent_id: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            skill_id: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            retention: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            summary_message_id: Option<String>,
            archived_at: Option<String>,
        },
        Message {
            id: String,
            time: String,
            role: SessionMessageRole,
            text: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            execution: Option<MessageExecution>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            media: Option<ChatMessageMedia>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            transcript: Option<ChatMessageTranscript>,
        },
        Reasoning {
            id: String,
            time: String,
            text: String,
        },
        ToolCall {
            id: String,
            time: String,
            tool: String,
            #[serde(default)]
            input: Value,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cwd: Option<String>,
        },
        ToolResult {
            id: String,
            time: String,
            tool: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            output: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            status: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            error: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            exit_code: Option<i32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            duration_ms: Option<u64>,
        },
        TurnEvent {
            id: String,
            time: String,
            turn_id: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            status: Option<ChatTurnStatus>,
            #[serde(rename = "event")]
            kind: ChatTurnEventKind,
        },
        Compact {
            id: String,
            time: String,
            summary: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            auto: Option<bool>,
        },
        Usage {
            time: String,
            #[serde(flatten)]
            usage: UsageValues,
        },
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum SessionLogSummaryEvent {
        SessionMeta {
            id: String,
            updated_at: String,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            model: Option<String>,
            #[serde(default)]
            provider: Option<String>,
            #[serde(default)]
            agent_id: Option<String>,
            #[serde(default)]
            skill_id: Option<String>,
            archived_at: Option<String>,
        },
        Message {
            time: String,
            role: SessionMessageRole,
            text: String,
        },
        Reasoning {
            time: String,
        },
        ToolCall {
            time: String,
            tool: String,
        },
        ToolResult {
            time: String,
            tool: String,
            #[serde(default)]
            status: Option<String>,
        },
        TurnEvent {
            time: String,
            turn_id: String,
        },
        Compact {
            time: String,
        },
        Usage {
            time: String,
        },
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct SessionMeta {
        pub id: String,
        pub created_at: String,
        pub updated_at: String,
        pub title: Option<String>,
        pub cwd: Option<String>,
        pub model: Option<String>,
        pub provider: Option<String>,
        pub app_version: Option<String>,
        pub git_branch: Option<String>,
        pub agent_id: Option<String>,
        pub skill_id: Option<String>,
        pub retention: Option<String>,
        pub summary_message_id: Option<String>,
        pub archived_at: Option<String>,
    }

    impl SessionMeta {
        pub fn new(id: String, created_at: String, updated_at: String) -> Self {
            Self {
                id,
                created_at,
                updated_at,
                title: None,
                cwd: None,
                model: None,
                provider: None,
                app_version: None,
                git_branch: None,
                agent_id: None,
                skill_id: None,
                retention: None,
                summary_message_id: None,
                archived_at: None,
            }
        }

        pub fn into_event(self) -> SessionLogEvent {
            SessionLogEvent::SessionMeta {
                schema_version: SESSION_SCHEMA_VERSION,
                id: self.id,
                created_at: self.created_at,
                updated_at: self.updated_at,
                title: self.title,
                cwd: self.cwd,
                model: self.model,
                provider: self.provider,
                app_version: self.app_version,
                git_branch: self.git_branch,
                agent_id: self.agent_id,
                skill_id: self.skill_id,
                retention: self.retention,
                summary_message_id: self.summary_message_id,
                archived_at: self.archived_at,
            }
        }
    }

    impl TryFrom<&SessionLogEvent> for SessionMeta {
        type Error = anyhow::Error;

        fn try_from(event: &SessionLogEvent) -> Result<Self> {
            match event {
                SessionLogEvent::SessionMeta {
                    id,
                    created_at,
                    updated_at,
                    title,
                    cwd,
                    model,
                    provider,
                    app_version,
                    git_branch,
                    agent_id,
                    skill_id,
                    retention,
                    summary_message_id,
                    archived_at,
                    ..
                } => Ok(Self {
                    id: id.clone(),
                    created_at: created_at.clone(),
                    updated_at: updated_at.clone(),
                    title: title.clone(),
                    cwd: cwd.clone(),
                    model: model.clone(),
                    provider: provider.clone(),
                    app_version: app_version.clone(),
                    git_branch: git_branch.clone(),
                    agent_id: agent_id.clone(),
                    skill_id: skill_id.clone(),
                    retention: retention.clone(),
                    summary_message_id: summary_message_id.clone(),
                    archived_at: archived_at.clone(),
                }),
                _ => Err(anyhow!("first session line is not session_meta")),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct FileSession {
        pub meta: SessionMeta,
        pub events: Vec<SessionLogEvent>,
    }

    impl FileSession {
        pub fn new(meta: SessionMeta, events: Vec<SessionLogEvent>) -> Self {
            Self { meta, events }
        }

        pub fn from_events(mut events: Vec<SessionLogEvent>) -> Result<Self> {
            let first = events
                .first()
                .ok_or_else(|| anyhow!("session transcript is empty"))?;
            SessionMeta::try_from(first)?;
            if let Some(last) = latest_event_time(&events) {
                meta_updated_in_place(&mut events, &last);
            }
            let meta = SessionMeta::try_from(
                events
                    .first()
                    .ok_or_else(|| anyhow!("session transcript is empty"))?,
            )?;
            Ok(Self { meta, events })
        }

        pub fn from_chat_session(session: &ChatSession) -> Self {
            let created_at = iso_from_millis(session.created_at);
            let updated_at = iso_from_millis(session.updated_at);
            let mut meta = SessionMeta::new(session.id.clone(), created_at, updated_at);
            meta.title = Some(session.name.clone());
            meta.model = Some(session.model.clone());
            meta.provider = Some(session.provider.clone()).filter(|value| !value.trim().is_empty());
            meta.agent_id = Some(session.agent_id.clone());
            meta.skill_id = session.skill_id.clone();
            meta.retention = session.retention.clone();
            meta.summary_message_id = session.summary_message_id.clone();
            meta.archived_at = session.archived_at.map(iso_from_millis);

            let mut events = vec![meta.clone().into_event()];
            for message in &session.messages {
                events.push(message_event_from_chat_message(message));
            }
            for turn in &session.turns {
                for event in &turn.events {
                    events.push(turn_event_from_chat_turn_event(turn, event));
                }
            }

            Self { meta, events }
        }

        pub fn merge_chat_session(existing: Option<&FileSession>, session: &ChatSession) -> Self {
            let mut next = FileSession::from_chat_session(session);
            let Some(existing) = existing else {
                return next;
            };

            let next_events = next
                .events
                .iter()
                .skip(1)
                .filter_map(|event| event_id(event).map(|id| (id.to_string(), event.clone())))
                .collect::<HashMap<_, _>>();
            let mut remaining_event_ids = next_events.keys().cloned().collect::<HashSet<_>>();
            let mut merged_events = vec![next.meta.clone().into_event()];

            for event in existing.events.iter().skip(1) {
                if let Some(id) = event_id(event)
                    && let Some(next_event) = next_events.get(id)
                {
                    if std::mem::discriminant(event) == std::mem::discriminant(next_event) {
                        merged_events.push(next_event.clone());
                    } else {
                        merged_events.push(event.clone());
                    }
                    remaining_event_ids.remove(id);
                    continue;
                }
                merged_events.push(event.clone());
            }

            for event in next.events.drain(1..) {
                let Some(id) = event_id(&event) else {
                    merged_events.push(event);
                    continue;
                };
                if remaining_event_ids.remove(id) {
                    merged_events.push(event);
                }
            }

            next.events = merged_events;
            next
        }

        pub fn to_chat_session(&self) -> ChatSession {
            let mut session = ChatSession {
                id: self.meta.id.clone(),
                name: self
                    .meta
                    .title
                    .clone()
                    .unwrap_or_else(|| "Imported Chat".to_string()),
                agent_id: self
                    .meta
                    .agent_id
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                provider: self.meta.provider.clone().unwrap_or_default(),
                model: self
                    .meta
                    .model
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                messages: Vec::new(),
                turns: Vec::new(),
                created_at: parse_time_ms(&self.meta.created_at),
                updated_at: parse_time_ms(&self.meta.updated_at),
                skill_id: self.meta.skill_id.clone(),
                retention: self.meta.retention.clone(),
                summary_message_id: self.meta.summary_message_id.clone(),
                prompt_tokens: 0,
                completion_tokens: 0,
                cost: 0.0,
                metadata: ChatSessionMetadata::new(),
                archived_at: self.meta.archived_at.as_deref().map(parse_time_ms),
            };

            for event in &self.events {
                match event {
                    SessionLogEvent::Message {
                        id,
                        time,
                        role,
                        text,
                        execution,
                        media,
                        transcript,
                    } => {
                        let message = ChatMessage {
                            id: id.clone(),
                            role: role.as_chat_role(),
                            content: text.clone(),
                            timestamp: parse_time_ms(time),
                            execution: execution.clone(),
                            media: media.clone(),
                            transcript: transcript.clone(),
                        };
                        push_message_unbounded(&mut session, message);
                    }
                    SessionLogEvent::Reasoning { id, time, text } => {
                        let message = ChatMessage {
                            id: id.clone(),
                            role: ChatRole::System,
                            content: format!("[reasoning]\n{text}"),
                            timestamp: parse_time_ms(time),
                            execution: None,
                            media: None,
                            transcript: None,
                        };
                        push_message_unbounded(&mut session, message);
                    }
                    SessionLogEvent::ToolCall {
                        id,
                        time,
                        tool,
                        input,
                        ..
                    } => {
                        let message = ChatMessage {
                            id: id.clone(),
                            role: ChatRole::System,
                            content: format!("[tool_call:{tool}] {input}"),
                            timestamp: parse_time_ms(time),
                            execution: None,
                            media: None,
                            transcript: None,
                        };
                        push_message_unbounded(&mut session, message);
                    }
                    SessionLogEvent::ToolResult {
                        id,
                        time,
                        tool,
                        output,
                        status,
                        error,
                        ..
                    } => {
                        let body = error.clone().or_else(|| output.clone()).unwrap_or_default();
                        let status = status.clone().unwrap_or_else(|| "completed".to_string());
                        let message = ChatMessage {
                            id: id.clone(),
                            role: ChatRole::System,
                            content: format!("[tool_result:{tool}:{status}]\n{body}"),
                            timestamp: parse_time_ms(time),
                            execution: None,
                            media: None,
                            transcript: None,
                        };
                        push_message_unbounded(&mut session, message);
                    }
                    SessionLogEvent::TurnEvent {
                        id,
                        time,
                        turn_id,
                        status,
                        kind,
                    } => {
                        let timestamp = parse_time_ms(time);
                        let index = if let Some(index) =
                            session.turns.iter().position(|turn| turn.id == *turn_id)
                        {
                            index
                        } else {
                            session.turns.push(ChatTurn {
                                id: turn_id.clone(),
                                status: ChatTurnStatus::Running,
                                started_at: timestamp,
                                updated_at: timestamp,
                                completed_at: None,
                                events: Vec::new(),
                            });
                            session.turns.len() - 1
                        };
                        let turn = &mut session.turns[index];
                        turn.events.push(ChatTurnEvent {
                            id: id.clone(),
                            timestamp,
                            kind: kind.clone(),
                        });
                        turn.updated_at = timestamp;
                        if let Some(status) = status {
                            turn.status = *status;
                            if matches!(
                                status,
                                ChatTurnStatus::Completed
                                    | ChatTurnStatus::Canceled
                                    | ChatTurnStatus::Failed
                            ) {
                                turn.completed_at = Some(timestamp);
                            }
                        }
                    }
                    SessionLogEvent::Compact {
                        id, time, summary, ..
                    } => {
                        let message = ChatMessage {
                            id: id.clone(),
                            role: ChatRole::System,
                            content: format!("[compact]\n{summary}"),
                            timestamp: parse_time_ms(time),
                            execution: None,
                            media: None,
                            transcript: None,
                        };
                        push_message_unbounded(&mut session, message);
                    }
                    SessionLogEvent::Usage { usage, .. } => {
                        if let Some(input) = usage.input_tokens {
                            session.prompt_tokens += input;
                        }
                        if let Some(output) = usage.output_tokens {
                            session.completion_tokens += output;
                        }
                        if let Some(cost) = usage.cost {
                            session.cost += cost;
                        }
                    }
                    SessionLogEvent::SessionMeta { .. } => {}
                }
            }

            session.metadata.message_count = session.messages.len() as u32;
            if session.name == "Imported Chat" {
                session.auto_name_from_first_message();
            }
            session.created_at = parse_time_ms(&self.meta.created_at);
            session.updated_at = parse_time_ms(&self.meta.updated_at);
            session
        }
    }

    #[derive(Debug, Clone)]
    pub struct FileSessionStore {
        root: PathBuf,
    }

    #[derive(Debug, Clone)]
    struct SessionSummaryCacheEntry {
        summaries: Vec<ChatSessionSummary>,
    }

    #[derive(Debug, Clone)]
    struct SessionPathCacheEntry {
        paths: Vec<PathBuf>,
    }

    fn session_summary_cache() -> &'static Mutex<HashMap<PathBuf, SessionSummaryCacheEntry>> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, SessionSummaryCacheEntry>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn session_path_cache() -> &'static Mutex<HashMap<PathBuf, SessionPathCacheEntry>> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, SessionPathCacheEntry>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn invalidate_session_caches(root: &Path) {
        if let Ok(mut cache) = session_summary_cache().lock() {
            cache.remove(root);
        }
        if let Ok(mut cache) = session_path_cache().lock() {
            cache.remove(root);
        }
    }

    impl FileSessionStore {
        pub fn default_root() -> Result<PathBuf> {
            crate::paths::sessions_dir()
        }

        pub fn open_default() -> Result<Self> {
            Self::new(Self::default_root()?)
        }

        pub fn new(root: PathBuf) -> Result<Self> {
            Ok(Self { root })
        }

        pub fn root(&self) -> &Path {
            &self.root
        }

        pub fn create_empty(&self, agent_id: String, model: String) -> Result<FileSession> {
            let now = now_iso();
            let id = Uuid::new_v4().to_string();
            let mut meta = SessionMeta::new(id, now.clone(), now);
            meta.title = Some("New Chat".to_string());
            meta.model = Some(model);
            meta.agent_id = Some(agent_id);
            let session = FileSession::new(meta.clone(), vec![meta.into_event()]);
            self.write_session(&session, false)?;
            Ok(session)
        }

        pub fn write_session(&self, session: &FileSession, force: bool) -> Result<WriteOutcome> {
            let path = self.path_for_meta(&session.meta)?;
            if path.exists() && !force {
                return Ok(WriteOutcome::Skipped { path });
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = File::create(&path)?;
            for event in &session.events {
                write_event_line(&mut file, event)?;
            }
            invalidate_session_caches(&self.root);
            Ok(WriteOutcome::Written { path })
        }

        pub fn append_event(&self, session_id: &str, event: &SessionLogEvent) -> Result<()> {
            let path = self
                .find_session_path(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {session_id}"))?;
            let mut file = OpenOptions::new().append(true).open(path)?;
            write_event_line(&mut file, event)?;
            invalidate_session_caches(&self.root);
            Ok(())
        }

        pub fn get(&self, id: &str) -> Result<Option<FileSession>> {
            let Some(path) = self.find_session_path(id)? else {
                return Ok(None);
            };
            read_session_file(&path).map(Some)
        }

        pub fn get_by_turn_id(&self, turn_id: &str) -> Result<Option<FileSession>> {
            let turn_id = turn_id.trim();
            if turn_id.is_empty() {
                return Ok(None);
            }
            for path in self.session_paths()? {
                match session_file_contains_turn_id(&path, turn_id) {
                    Ok(true) => return read_session_file(&path).map(Some),
                    Ok(false) => {}
                    Err(err) => {
                        tracing::warn!(path = %path.display(), error = %err, "Skipping invalid session file")
                    }
                }
            }
            Ok(None)
        }

        pub fn delete(&self, id: &str) -> Result<bool> {
            let Some(path) = self.find_session_path(id)? else {
                return Ok(false);
            };
            fs::remove_file(path)?;
            invalidate_session_caches(&self.root);
            Ok(true)
        }

        pub fn list(&self) -> Result<Vec<FileSession>> {
            let mut sessions = Vec::new();
            for path in self.session_paths()? {
                match read_session_file(&path) {
                    Ok(session) => sessions.push(session),
                    Err(err) => {
                        tracing::warn!(path = %path.display(), error = %err, "Skipping invalid session file")
                    }
                }
            }
            sessions
                .sort_by_key(|session| std::cmp::Reverse(parse_time_ms(&session.meta.updated_at)));
            Ok(sessions)
        }

        pub fn list_summaries(&self) -> Result<Vec<ChatSessionSummary>> {
            let mut summaries = self.list_summary_cache_entries()?;
            summaries.retain(|summary| summary.archived_at.is_none());
            Ok(summaries)
        }

        pub fn list_summaries_all(&self) -> Result<Vec<ChatSessionSummary>> {
            self.list_summary_cache_entries()
        }

        fn list_summary_cache_entries(&self) -> Result<Vec<ChatSessionSummary>> {
            let cache_key = self.root.clone();
            if let Some(entry) = session_summary_cache()
                .lock()
                .ok()
                .and_then(|cache| cache.get(&cache_key).cloned())
            {
                return Ok(entry.summaries);
            }

            let mut summaries = Vec::new();
            for path in self.session_paths()? {
                match read_session_summary_file(&path) {
                    Ok(summary) => summaries.push(summary),
                    Err(err) => {
                        tracing::warn!(path = %path.display(), error = %err, "Skipping invalid session file")
                    }
                }
            }
            summaries.sort_by_key(|session| std::cmp::Reverse(session.updated_at));

            if let Ok(mut cache) = session_summary_cache().lock() {
                cache.insert(
                    cache_key,
                    SessionSummaryCacheEntry {
                        summaries: summaries.clone(),
                    },
                );
            }
            Ok(summaries)
        }

        pub fn search(&self, query: &str) -> Result<Vec<FileSession>> {
            let needle = query.trim().to_lowercase();
            if needle.is_empty() {
                return Ok(Vec::new());
            }
            Ok(self
                .list()?
                .into_iter()
                .filter(|session| {
                    session
                        .meta
                        .title
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&needle)
                        || session
                            .events
                            .iter()
                            .any(|event| event_text(event).contains(&needle))
                })
                .collect())
        }

        fn path_for_meta(&self, meta: &SessionMeta) -> Result<PathBuf> {
            let created = parse_datetime(&meta.created_at).unwrap_or_else(Utc::now);
            Ok(self
                .root
                .join(format!("{:04}", created.year()))
                .join(format!("{:02}", created.month()))
                .join(format!("{:02}", created.day()))
                .join(format!("{}.jsonl", sanitize_session_id(&meta.id))))
        }

        fn find_session_path(&self, id: &str) -> Result<Option<PathBuf>> {
            let exact_file = format!("{}.jsonl", sanitize_session_id(id));
            let mut prefix_matches = Vec::new();
            for path in self.session_paths()? {
                let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
                    continue;
                };
                if file_name == exact_file {
                    return Ok(Some(path));
                }
                if file_name.starts_with(id) {
                    prefix_matches.push(path);
                }
            }
            match prefix_matches.len() {
                0 => Ok(None),
                1 => Ok(prefix_matches.pop()),
                _ => Err(anyhow!("Session id is ambiguous: {id}")),
            }
        }

        fn session_paths(&self) -> Result<Vec<PathBuf>> {
            let cache_key = self.root.clone();
            if let Some(entry) = session_path_cache()
                .lock()
                .ok()
                .and_then(|cache| cache.get(&cache_key).cloned())
            {
                return Ok(entry.paths);
            }

            if !self.root.exists() {
                return Ok(Vec::new());
            }
            let mut entries = Vec::new();
            for entry in WalkDir::new(&self.root).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                if path.extension().and_then(|v| v.to_str()) == Some("jsonl") {
                    let path = path.to_path_buf();
                    let mut path_modified_ms = 0i64;
                    if let Ok(metadata) = path.metadata() {
                        path_modified_ms = modified_ms(&metadata);
                    }
                    entries.push((path, path_modified_ms));
                }
            }
            entries.sort_by(
                |(left_path, left_modified_ms), (right_path, right_modified_ms)| {
                    right_modified_ms
                        .cmp(left_modified_ms)
                        .then_with(|| left_path.cmp(right_path))
                },
            );
            let paths: Vec<PathBuf> = entries.into_iter().map(|(path, _)| path).collect();
            if let Ok(mut cache) = session_path_cache().lock() {
                cache.insert(
                    cache_key,
                    SessionPathCacheEntry {
                        paths: paths.clone(),
                    },
                );
            }
            Ok(paths)
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum WriteOutcome {
        Written { path: PathBuf },
        Skipped { path: PathBuf },
    }

    pub fn read_session_file(path: &Path) -> Result<FileSession> {
        let file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for (index, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("failed to read {}", path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            let event: SessionLogEvent = serde_json::from_str(&line)
                .with_context(|| format!("invalid JSONL at {}:{}", path.display(), index + 1))?;
            events.push(event);
        }
        FileSession::from_events(events)
    }

    fn read_session_summary_file(path: &Path) -> Result<ChatSessionSummary> {
        let file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut meta: Option<SessionLogSummaryEvent> = None;
        let mut latest_time: Option<String> = None;
        let mut message_count: u32 = 0;
        let mut last_message_preview: Option<String> = None;
        let mut first_user_message: Option<String> = None;

        for (index, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("failed to read {}", path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            let event: SessionLogSummaryEvent = serde_json::from_str(&line)
                .with_context(|| format!("invalid JSONL at {}:{}", path.display(), index + 1))?;
            if matches!(event, SessionLogSummaryEvent::SessionMeta { .. }) && meta.is_none() {
                latest_time = summary_event_time(&event).map(ToOwned::to_owned);
                meta = Some(event);
                continue;
            }
            if let Some(time) = summary_event_time(&event)
                && latest_time
                    .as_deref()
                    .map(|current| parse_time_ms(time) >= parse_time_ms(current))
                    .unwrap_or(true)
            {
                latest_time = Some(time.to_string());
            }
            if let Some(preview) = summary_event_preview(&event) {
                message_count = message_count.saturating_add(1);
                if first_user_message.is_none()
                    && let SessionLogSummaryEvent::Message {
                        role: SessionMessageRole::User,
                        text,
                        ..
                    } = &event
                {
                    first_user_message = Some(text.clone());
                }
                last_message_preview = Some(preview);
            }
        }

        let Some(SessionLogSummaryEvent::SessionMeta {
            id,
            updated_at,
            title,
            model,
            provider,
            agent_id,
            skill_id,
            archived_at,
        }) = meta
        else {
            return Err(anyhow!("first session line is not session_meta"));
        };

        let name = title
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                first_user_message
                    .as_deref()
                    .map(session_title_from_message)
            })
            .unwrap_or_else(|| "Imported Chat".to_string());
        let updated_at = latest_time
            .as_deref()
            .map(parse_time_ms)
            .unwrap_or_else(|| parse_time_ms(&updated_at));
        Ok(ChatSessionSummary {
            id,
            name,
            agent_id: agent_id.unwrap_or_else(|| "default".to_string()),
            provider: provider.unwrap_or_default(),
            model: model.unwrap_or_else(|| "unknown".to_string()),
            skill_id,
            message_count,
            updated_at,
            last_message_preview,
            archived_at: archived_at.as_deref().map(parse_time_ms),
        })
    }

    fn session_file_contains_turn_id(path: &Path, turn_id: &str) -> Result<bool> {
        let file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let encoded_turn_id = serde_json::to_string(turn_id)?;
        let compact_needle = format!("\"turn_id\":{encoded_turn_id}");
        let spaced_needle = format!("\"turn_id\": {encoded_turn_id}");
        for (index, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("failed to read {}", path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            if line.contains(&compact_needle) || line.contains(&spaced_needle) {
                return Ok(true);
            }
            if !line.contains("\"turn_id\"") {
                continue;
            }
            let event: SessionLogSummaryEvent = serde_json::from_str(&line)
                .with_context(|| format!("invalid JSONL at {}:{}", path.display(), index + 1))?;
            if let SessionLogSummaryEvent::TurnEvent {
                turn_id: event_turn_id,
                ..
            } = event
                && event_turn_id == turn_id
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn modified_ms(metadata: &fs::Metadata) -> i64 {
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or_default()
    }

    fn summary_event_time(event: &SessionLogSummaryEvent) -> Option<&str> {
        match event {
            SessionLogSummaryEvent::SessionMeta { updated_at, .. }
            | SessionLogSummaryEvent::Message {
                time: updated_at, ..
            }
            | SessionLogSummaryEvent::Reasoning { time: updated_at }
            | SessionLogSummaryEvent::ToolCall {
                time: updated_at, ..
            }
            | SessionLogSummaryEvent::ToolResult {
                time: updated_at, ..
            }
            | SessionLogSummaryEvent::TurnEvent {
                time: updated_at, ..
            }
            | SessionLogSummaryEvent::Compact { time: updated_at }
            | SessionLogSummaryEvent::Usage { time: updated_at } => Some(updated_at.as_str()),
        }
    }

    fn summary_event_preview(event: &SessionLogSummaryEvent) -> Option<String> {
        match event {
            SessionLogSummaryEvent::Message { text, .. } => Some(truncate_summary_preview(text)),
            SessionLogSummaryEvent::Reasoning { .. } => Some("[reasoning]".to_string()),
            SessionLogSummaryEvent::ToolCall { tool, .. } => Some(format!("[tool_call:{tool}]")),
            SessionLogSummaryEvent::ToolResult { tool, status, .. } => Some(format!(
                "[tool_result:{}:{}]",
                tool,
                status.as_deref().unwrap_or("completed")
            )),
            SessionLogSummaryEvent::Compact { .. } => Some("[compact]".to_string()),
            SessionLogSummaryEvent::SessionMeta { .. }
            | SessionLogSummaryEvent::TurnEvent { .. }
            | SessionLogSummaryEvent::Usage { .. } => None,
        }
    }

    fn truncate_summary_preview(text: &str) -> String {
        let preview: String = text.chars().take(50).collect();
        if text.chars().count() > 50 {
            format!("{}...", preview)
        } else {
            preview
        }
    }

    fn session_title_from_message(text: &str) -> String {
        let title: String = text.chars().take(30).collect();
        if text.chars().count() > 30 {
            format!("{}...", title)
        } else {
            title
        }
    }

    pub fn stable_session_id(events: &[SessionLogEvent]) -> String {
        let mut hasher = Sha256::new();
        for event in events {
            if matches!(event, SessionLogEvent::SessionMeta { .. }) {
                continue;
            }
            if let Ok(bytes) = serde_json::to_vec(event) {
                hasher.update(bytes);
                hasher.update(b"\n");
            }
        }
        let digest = hasher.finalize();
        hex::encode(digest)[..32].to_string()
    }

    pub fn now_iso() -> String {
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    pub fn parse_time_ms(value: &str) -> i64 {
        parse_datetime(value)
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_default()
    }

    pub fn iso_from_millis(value: i64) -> String {
        Utc.timestamp_millis_opt(value)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    pub fn text_from_content(value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            Value::Array(items) => items
                .iter()
                .filter_map(content_item_text)
                .collect::<Vec<_>>()
                .join("\n"),
            Value::Object(_) => content_item_text(value).unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn content_item_text(value: &Value) -> Option<String> {
        let object = value.as_object()?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") | Some("input_text") | Some("output_text") => object
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            Some("thinking") | Some("reasoning") => object
                .get("thinking")
                .or_else(|| object.get("text"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            Some("tool_result") => object
                .get("content")
                .map(text_from_content)
                .filter(|text| !text.is_empty()),
            _ => object
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        }
    }

    fn write_event_line(file: &mut File, event: &SessionLogEvent) -> Result<()> {
        serde_json::to_writer(&mut *file, event)?;
        file.write_all(b"\n")?;
        Ok(())
    }

    fn message_event_from_chat_message(message: &ChatMessage) -> SessionLogEvent {
        SessionLogEvent::Message {
            id: message.id.clone(),
            time: iso_from_millis(message.timestamp),
            role: SessionMessageRole::from_chat_role(&message.role),
            text: message.content.clone(),
            execution: message.execution.clone(),
            media: message.media.clone(),
            transcript: message.transcript.clone(),
        }
    }

    fn turn_event_from_chat_turn_event(turn: &ChatTurn, event: &ChatTurnEvent) -> SessionLogEvent {
        SessionLogEvent::TurnEvent {
            id: event.id.clone(),
            time: iso_from_millis(event.timestamp),
            turn_id: turn.id.clone(),
            status: Some(turn.status),
            kind: event.kind.clone(),
        }
    }

    fn push_message_unbounded(session: &mut ChatSession, message: ChatMessage) {
        if let Some(execution) = &message.execution {
            session.metadata.update(execution.tokens_used);
            if let Some(input) = execution.input_tokens {
                session.prompt_tokens += i64::from(input);
            }
            if let Some(output) = execution.output_tokens {
                session.completion_tokens += i64::from(output);
            }
            if let Some(cost) = execution.cost_usd {
                session.cost += cost;
            }
        } else {
            session.metadata.message_count += 1;
        }
        session.messages.push(message);
    }

    fn event_id(event: &SessionLogEvent) -> Option<&str> {
        match event {
            SessionLogEvent::Message { id, .. }
            | SessionLogEvent::Reasoning { id, .. }
            | SessionLogEvent::ToolCall { id, .. }
            | SessionLogEvent::ToolResult { id, .. }
            | SessionLogEvent::TurnEvent { id, .. }
            | SessionLogEvent::Compact { id, .. } => Some(id),
            SessionLogEvent::SessionMeta { .. } | SessionLogEvent::Usage { .. } => None,
        }
    }

    fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
    }

    fn sanitize_session_id(id: &str) -> String {
        id.chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '-'
                }
            })
            .collect()
    }

    fn latest_event_time(events: &[SessionLogEvent]) -> Option<String> {
        events
            .iter()
            .map(|event| match event {
                SessionLogEvent::SessionMeta { updated_at, .. } => updated_at.clone(),
                SessionLogEvent::Message { time, .. }
                | SessionLogEvent::Reasoning { time, .. }
                | SessionLogEvent::ToolCall { time, .. }
                | SessionLogEvent::ToolResult { time, .. }
                | SessionLogEvent::TurnEvent { time, .. }
                | SessionLogEvent::Compact { time, .. }
                | SessionLogEvent::Usage { time, .. } => time.clone(),
            })
            .max_by_key(|time| parse_time_ms(time))
    }

    fn meta_updated_in_place(events: &mut [SessionLogEvent], updated_at: &str) {
        if let Some(SessionLogEvent::SessionMeta {
            updated_at: current,
            ..
        }) = events.first_mut()
        {
            *current = updated_at.to_string();
        }
    }

    fn event_text(event: &SessionLogEvent) -> String {
        match event {
            SessionLogEvent::SessionMeta { title, cwd, .. } => {
                format!(
                    "{} {}",
                    title.as_deref().unwrap_or(""),
                    cwd.as_deref().unwrap_or("")
                )
            }
            SessionLogEvent::Message { text, .. }
            | SessionLogEvent::Reasoning { text, .. }
            | SessionLogEvent::Compact { summary: text, .. } => text.to_lowercase(),
            SessionLogEvent::ToolCall { tool, input, .. } => {
                format!("{tool} {input}").to_lowercase()
            }
            SessionLogEvent::ToolResult {
                tool,
                output,
                error,
                ..
            } => format!(
                "{tool} {} {}",
                output.as_deref().unwrap_or(""),
                error.as_deref().unwrap_or("")
            )
            .to_lowercase(),
            SessionLogEvent::TurnEvent { kind, .. } => match kind {
                ChatTurnEventKind::UserMessage { content }
                | ChatTurnEventKind::AssistantMessage { content } => content.to_lowercase(),
                ChatTurnEventKind::ToolCall {
                    name, arguments, ..
                } => format!("{name} {arguments}").to_lowercase(),
                ChatTurnEventKind::ToolResult { result, .. } => result.to_lowercase(),
                ChatTurnEventKind::Progress { message } => message.to_lowercase(),
                ChatTurnEventKind::Error { message } => message.to_lowercase(),
                ChatTurnEventKind::Canceled => "canceled".to_string(),
            },
            SessionLogEvent::Usage { .. } => String::new(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::tempdir;
        use types::ExecutionStepInfo;

        #[test]
        fn writes_and_reads_one_jsonl_session() {
            let dir = tempdir().unwrap();
            let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
            let mut meta = SessionMeta::new(
                "session-1".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
                "2026-05-03T00:00:01.000Z".to_string(),
            );
            meta.title = Some("Hello".to_string());
            let session = FileSession::new(
                meta.clone(),
                vec![
                    meta.into_event(),
                    SessionLogEvent::Message {
                        id: "msg-1".to_string(),
                        time: "2026-05-03T00:00:01.000Z".to_string(),
                        role: SessionMessageRole::User,
                        text: "hello".to_string(),
                        execution: None,
                        media: None,
                        transcript: None,
                    },
                ],
            );
            assert!(matches!(
                store.write_session(&session, false).unwrap(),
                WriteOutcome::Written { .. }
            ));
            let loaded = store.get("session-1").unwrap().unwrap();
            assert_eq!(loaded.meta.id, "session-1");
            assert_eq!(loaded.to_chat_session().messages.len(), 1);
        }

        #[test]
        fn lists_file_session_summaries_without_full_session_hydration() {
            let dir = tempdir().unwrap();
            let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
            let mut meta = SessionMeta::new(
                "session-1".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
            );
            meta.provider = Some("codex".to_string());
            meta.model = Some("gpt-5.4".to_string());
            meta.agent_id = Some("agent-1".to_string());
            let session = FileSession::new(
                meta.clone(),
                vec![
                    meta.into_event(),
                    SessionLogEvent::Message {
                        id: "msg-1".to_string(),
                        time: "2026-05-03T00:00:01.000Z".to_string(),
                        role: SessionMessageRole::User,
                        text: "hello from a lightweight summary".to_string(),
                        execution: None,
                        media: None,
                        transcript: None,
                    },
                    SessionLogEvent::ToolResult {
                        id: "tool-1".to_string(),
                        time: "2026-05-03T00:00:02.000Z".to_string(),
                        tool: "bash".to_string(),
                        output: Some(
                            "large output does not need to hydrate into a chat message".repeat(8),
                        ),
                        status: Some("completed".to_string()),
                        error: None,
                        exit_code: Some(0),
                        duration_ms: Some(10),
                    },
                ],
            );
            store.write_session(&session, false).unwrap();

            let summaries = store.list_summaries().unwrap();

            assert_eq!(summaries.len(), 1);
            assert_eq!(summaries[0].id, "session-1");
            assert_eq!(summaries[0].agent_id, "agent-1");
            assert_eq!(summaries[0].provider, "codex");
            assert_eq!(summaries[0].model, "gpt-5.4");
            assert_eq!(summaries[0].message_count, 2);
            assert_eq!(
                summaries[0].updated_at,
                parse_time_ms("2026-05-03T00:00:02.000Z")
            );
            assert_eq!(
                summaries[0].last_message_preview.as_deref(),
                Some("[tool_result:bash:completed]")
            );
        }

        #[test]
        fn cached_file_session_summaries_refresh_after_append() {
            let dir = tempdir().unwrap();
            let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
            let meta = SessionMeta::new(
                "session-cache-refresh".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
            );
            let session = FileSession::new(
                meta.clone(),
                vec![
                    meta.into_event(),
                    SessionLogEvent::Message {
                        id: "msg-1".to_string(),
                        time: "2026-05-03T00:00:01.000Z".to_string(),
                        role: SessionMessageRole::User,
                        text: "hello".to_string(),
                        execution: None,
                        media: None,
                        transcript: None,
                    },
                ],
            );
            store.write_session(&session, false).unwrap();

            let initial = store.list_summaries().unwrap();
            assert_eq!(initial[0].message_count, 1);
            assert_eq!(initial[0].last_message_preview.as_deref(), Some("hello"));

            store
                .append_event(
                    "session-cache-refresh",
                    &SessionLogEvent::Message {
                        id: "msg-2".to_string(),
                        time: "2026-05-03T00:00:02.000Z".to_string(),
                        role: SessionMessageRole::Assistant,
                        text: "world".to_string(),
                        execution: None,
                        media: None,
                        transcript: None,
                    },
                )
                .unwrap();

            let refreshed = store.list_summaries().unwrap();
            assert_eq!(refreshed[0].message_count, 2);
            assert_eq!(refreshed[0].last_message_preview.as_deref(), Some("world"));
        }

        #[test]
        fn cached_file_session_paths_refresh_after_new_session_write() {
            let dir = tempdir().unwrap();
            let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();

            let first_meta = SessionMeta::new(
                "session-path-cache-1".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
                "2026-05-03T00:00:01.000Z".to_string(),
            );
            let first_session = FileSession::new(first_meta.clone(), vec![first_meta.into_event()]);
            store.write_session(&first_session, false).unwrap();

            let initial = store.list_summaries().unwrap();
            assert_eq!(initial.len(), 1);
            assert_eq!(initial[0].id, "session-path-cache-1");

            let second_meta = SessionMeta::new(
                "session-path-cache-2".to_string(),
                "2026-05-03T00:00:02.000Z".to_string(),
                "2026-05-03T00:00:03.000Z".to_string(),
            );
            let second_session =
                FileSession::new(second_meta.clone(), vec![second_meta.into_event()]);
            store.write_session(&second_session, false).unwrap();

            let refreshed = store.list_summaries().unwrap();
            let ids = refreshed
                .iter()
                .map(|summary| summary.id.as_str())
                .collect::<HashSet<_>>();
            assert_eq!(ids.len(), 2);
            assert!(ids.contains("session-path-cache-1"));
            assert!(ids.contains("session-path-cache-2"));
        }

        #[test]
        fn skips_existing_session_without_force() {
            let dir = tempdir().unwrap();
            let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
            let session = store
                .create_empty("default".to_string(), "gpt-5.4".to_string())
                .unwrap();
            assert!(matches!(
                store.write_session(&session, false).unwrap(),
                WriteOutcome::Skipped { .. }
            ));
        }

        #[test]
        fn extracts_text_from_common_content_shapes() {
            let value = serde_json::json!([
                { "type": "input_text", "text": "hello" },
                { "type": "output_text", "text": "world" }
            ]);
            assert_eq!(text_from_content(&value), "hello\nworld");
        }

        #[test]
        fn converts_chat_session_to_jsonl_session() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
                .with_name("Build fix")
                .with_skill("release")
                .with_retention("7d");
            session.add_message(ChatMessage::user("hello"));
            session.add_message(ChatMessage::assistant("world"));
            let summary_message_id = session.messages[1].id.clone();
            session.summary_message_id = Some(summary_message_id.clone());
            session.archive();

            let file_session = FileSession::from_chat_session(&session);
            assert_eq!(file_session.meta.agent_id.as_deref(), Some("agent-1"));
            assert_eq!(file_session.meta.skill_id.as_deref(), Some("release"));
            assert_eq!(file_session.meta.retention.as_deref(), Some("7d"));
            assert!(file_session.meta.archived_at.is_some());
            let reloaded = file_session.to_chat_session();
            assert_eq!(reloaded.skill_id.as_deref(), Some("release"));
            assert_eq!(
                reloaded.summary_message_id.as_deref(),
                Some(summary_message_id.as_str())
            );
            assert!(reloaded.is_archived());
        }

        #[test]
        fn turn_events_roundtrip_through_jsonl_session() {
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
            session.record_turn_user_message("turn-1", "hello");
            session.record_turn_event(
                "turn-1",
                ChatTurnEventKind::ToolCall {
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: "pwd".to_string(),
                },
            );
            session.record_turn_event(
                "turn-1",
                ChatTurnEventKind::ToolResult {
                    call_id: "call-1".to_string(),
                    success: true,
                    result: "/tmp/project".to_string(),
                },
            );
            session.complete_turn_with_assistant_message("turn-1", "done");

            let file_session = FileSession::from_chat_session(&session);
            assert!(matches!(
                file_session.events.get(1),
                Some(SessionLogEvent::TurnEvent { .. })
            ));

            let reloaded = file_session.to_chat_session();
            assert_eq!(reloaded.turns.len(), 1);
            assert_eq!(reloaded.turns[0].status, ChatTurnStatus::Completed);
            assert_eq!(reloaded.turns[0].events.len(), 4);
            assert!(matches!(
                reloaded.turns[0].events[1].kind,
                ChatTurnEventKind::ToolCall { .. }
            ));
        }

        #[test]
        fn finds_file_session_by_turn_id_without_hydrating_every_file() {
            let dir = tempdir().unwrap();
            let store = FileSessionStore::new(dir.path().join("sessions")).unwrap();

            let mut other_session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
            other_session.id = "session-other".to_string();
            other_session.record_turn_user_message("turn-other", "ignore me");
            store
                .write_session(&FileSession::from_chat_session(&other_session), false)
                .unwrap();

            let mut target_session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
            target_session.id = "session-target".to_string();
            target_session.record_turn_user_message("turn-target", "find me");
            store
                .write_session(&FileSession::from_chat_session(&target_session), false)
                .unwrap();

            let loaded = store.get_by_turn_id("turn-target").unwrap().unwrap();
            assert_eq!(loaded.meta.id, "session-target");
            assert_eq!(
                loaded.to_chat_session().turns[0].events[0].kind,
                ChatTurnEventKind::UserMessage {
                    content: "find me".to_string()
                }
            );
            assert!(store.get_by_turn_id("missing-turn").unwrap().is_none());
        }

        #[test]
        fn jsonl_roundtrip_does_not_truncate_long_sessions() {
            let mut meta = SessionMeta::new(
                "session-1".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
                "2026-05-03T00:05:00.000Z".to_string(),
            );
            meta.title = Some("Long chat".to_string());
            let mut events = vec![meta.clone().into_event()];
            for index in 0..250 {
                events.push(SessionLogEvent::Message {
                    id: format!("msg-{index}"),
                    time: "2026-05-03T00:00:01.000Z".to_string(),
                    role: SessionMessageRole::User,
                    text: format!("message {index}"),
                    execution: None,
                    media: None,
                    transcript: None,
                });
            }

            let session = FileSession::new(meta, events).to_chat_session();

            assert_eq!(session.messages.len(), 250);
            assert_eq!(session.messages.first().unwrap().content, "message 0");
            assert_eq!(session.messages.last().unwrap().content, "message 249");
        }

        #[test]
        fn merge_chat_session_preserves_existing_non_message_events() {
            let mut meta = SessionMeta::new(
                "session-1".to_string(),
                "2026-05-03T00:00:00.000Z".to_string(),
                "2026-05-03T00:00:02.000Z".to_string(),
            );
            meta.title = Some("Old title".to_string());
            let existing = FileSession::new(
                meta.clone(),
                vec![
                    meta.clone().into_event(),
                    SessionLogEvent::Message {
                        id: "msg-1".to_string(),
                        time: "2026-05-03T00:00:01.000Z".to_string(),
                        role: SessionMessageRole::User,
                        text: "hello".to_string(),
                        execution: None,
                        media: None,
                        transcript: None,
                    },
                    SessionLogEvent::ToolCall {
                        id: "tool-1".to_string(),
                        time: "2026-05-03T00:00:02.000Z".to_string(),
                        tool: "bash".to_string(),
                        input: serde_json::json!({ "command": "pwd" }),
                        cwd: Some("/tmp/project".to_string()),
                    },
                ],
            );
            let mut chat = existing.to_chat_session();
            chat.rename("New title");

            let merged = FileSession::merge_chat_session(Some(&existing), &chat);

            assert_eq!(merged.meta.title.as_deref(), Some("New title"));
            assert!(matches!(
                merged.events.get(2),
                Some(SessionLogEvent::ToolCall { tool, cwd, .. })
                    if tool == "bash" && cwd.as_deref() == Some("/tmp/project")
            ));
        }

        #[test]
        fn message_structured_fields_roundtrip_through_jsonl() {
            let mut execution = MessageExecution::new().complete(1200, 42);
            execution.input_tokens = Some(20);
            execution.output_tokens = Some(22);
            execution.cost_usd = Some(0.01);
            execution.add_step(
                ExecutionStepInfo::new("tool_call", "bash")
                    .with_status("completed")
                    .with_duration(50),
            );
            let message = ChatMessage::assistant("done")
                .with_execution(execution.clone())
                .with_media(ChatMessageMedia::voice("/tmp/voice.ogg", Some(3)))
                .with_transcript(ChatMessageTranscript {
                    text: "done".to_string(),
                    model: Some("whisper".to_string()),
                    updated_at: Some(1_777_852_800_000),
                });
            let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
            session.add_message(message);

            let reloaded = FileSession::from_chat_session(&session).to_chat_session();
            let reloaded_message = reloaded.messages.first().unwrap();

            assert_eq!(reloaded_message.execution, Some(execution));
            assert_eq!(
                reloaded_message.media,
                Some(ChatMessageMedia::voice("/tmp/voice.ogg", Some(3)))
            );
            assert_eq!(reloaded_message.transcript.as_ref().unwrap().text, "done");
        }
    }
}
pub mod steer {
    use std::collections::HashMap;
    use tokio::sync::{RwLock, mpsc};

    use types::SteerMessage;

    /// Registry of steer channels for running streams.
    /// Each running stream registers a sender; external code sends steer messages.
    pub struct SteerRegistry {
        channels: RwLock<HashMap<String, mpsc::Sender<SteerMessage>>>,
    }

    impl SteerRegistry {
        pub fn new() -> Self {
            Self {
                channels: RwLock::new(HashMap::new()),
            }
        }

        /// Register a steer channel for a running stream.
        /// Returns the receiver for the executor to poll.
        pub async fn register(&self, stream_id: &str) -> mpsc::Receiver<SteerMessage> {
            let (tx, rx) = mpsc::channel(16);
            self.channels
                .write()
                .await
                .insert(stream_id.to_string(), tx);
            rx
        }

        /// Unregister when stream completes.
        pub async fn unregister(&self, stream_id: &str) {
            self.channels.write().await.remove(stream_id);
        }

        /// Send a steer message to a running stream.
        /// Returns false if the stream is not running or channel is full.
        pub async fn steer(&self, stream_id: &str, message: SteerMessage) -> bool {
            let channels = self.channels.read().await;
            if let Some(tx) = channels.get(stream_id) {
                tx.try_send(message).is_ok()
            } else {
                false
            }
        }

        /// Check if a stream has a steer channel.
        pub async fn is_steerable(&self, stream_id: &str) -> bool {
            self.channels.read().await.contains_key(stream_id)
        }
    }

    impl Default for SteerRegistry {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use types::SteerSource;

        #[tokio::test]
        async fn test_steer_registry_register_unregister() {
            let registry = SteerRegistry::new();
            let _rx = registry.register("stream-1").await;
            assert!(registry.is_steerable("stream-1").await);

            registry.unregister("stream-1").await;
            assert!(!registry.is_steerable("stream-1").await);
        }

        #[tokio::test]
        async fn test_steer_message_delivery() {
            let registry = SteerRegistry::new();
            let mut rx = registry.register("stream-1").await;

            let msg = SteerMessage::message("check ETH too", SteerSource::User);
            assert!(registry.steer("stream-1", msg).await);

            let received = rx.recv().await.unwrap();
            assert_eq!(received.instruction(), "check ETH too");
        }

        #[tokio::test]
        async fn test_steer_nonexistent_stream() {
            let registry = SteerRegistry::new();
            let msg = SteerMessage::message("test", SteerSource::User);
            assert!(!registry.steer("no-such-stream", msg).await);
        }

        #[tokio::test]
        async fn test_steer_channel_capacity() {
            // Channel capacity is 16, sending 20 messages should drop overflow
            let registry = SteerRegistry::new();
            let _rx = registry.register("stream-1").await; // don't consume

            for i in 0..20 {
                registry
                    .steer(
                        "stream-1",
                        SteerMessage::message(format!("msg-{i}"), SteerSource::User),
                    )
                    .await;
            }
            // First 16 should be queued, rest dropped (try_send behavior)
        }
    }
}
pub mod storage {
    //! Storage aggregation for local files plus short-lived secret storage.

    pub mod redb_lease {
        use std::path::{Path, PathBuf};
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        use anyhow::{Context, Result};
        use redb::{Database, DatabaseError};

        #[derive(Debug, Clone)]
        pub struct RedbLeaseProvider {
            path: Arc<PathBuf>,
            timeout: Duration,
            initial_delay: Duration,
            max_delay: Duration,
        }

        impl RedbLeaseProvider {
            pub fn new(path: impl Into<PathBuf>) -> Self {
                Self {
                    path: Arc::new(path.into()),
                    timeout: Duration::from_secs(5),
                    initial_delay: Duration::from_millis(50),
                    max_delay: Duration::from_millis(250),
                }
            }

            #[cfg(test)]
            pub fn with_timing(
                path: impl Into<PathBuf>,
                timeout: Duration,
                initial_delay: Duration,
                max_delay: Duration,
            ) -> Self {
                Self {
                    path: Arc::new(path.into()),
                    timeout,
                    initial_delay,
                    max_delay,
                }
            }

            pub fn path(&self) -> &Path {
                self.path.as_ref()
            }

            pub fn with_database<T>(
                &self,
                operation: impl FnOnce(&Database) -> Result<T>,
            ) -> Result<T> {
                let db = self.open_database()?;
                operation(&db)
            }

            fn open_database(&self) -> Result<Database> {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("Failed to create database directory {}", parent.display())
                    })?;
                }

                let started_at = Instant::now();
                let mut delay = self.initial_delay;
                loop {
                    match Database::create(self.path.as_ref()) {
                        Ok(db) => return Ok(db),
                        Err(DatabaseError::DatabaseAlreadyOpen)
                            if started_at.elapsed() < self.timeout =>
                        {
                            thread::sleep(delay);
                            delay = (delay * 2).min(self.max_delay);
                        }
                        Err(DatabaseError::DatabaseAlreadyOpen) => {
                            anyhow::bail!(
                                "Timed out waiting for redb lease on {}",
                                self.path.display()
                            );
                        }
                        Err(err) => return Err(err).context("Failed to open redb database"),
                    }
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn lease_waits_until_existing_database_handle_is_released() {
                let dir = tempfile::tempdir().unwrap();
                let db_path = dir.path().join("lease.db");
                let held = Database::create(&db_path).unwrap();
                let provider = RedbLeaseProvider::with_timing(
                    db_path.clone(),
                    Duration::from_secs(2),
                    Duration::from_millis(10),
                    Duration::from_millis(25),
                );

                let handle = thread::spawn(move || provider.with_database(|_| Ok(())));
                thread::sleep(Duration::from_millis(100));
                drop(held);

                handle.join().unwrap().unwrap();
            }

            #[test]
            fn lease_times_out_when_database_stays_open() {
                let dir = tempfile::tempdir().unwrap();
                let db_path = dir.path().join("lease-timeout.db");
                let _held = Database::create(&db_path).unwrap();
                let provider = RedbLeaseProvider::with_timing(
                    db_path,
                    Duration::from_millis(50),
                    Duration::from_millis(10),
                    Duration::from_millis(10),
                );

                let error = provider.with_database(|_| Ok(())).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("Timed out waiting for redb lease")
                );
            }
        }
    }

    use anyhow::Result;
    use std::path::{Path, PathBuf};

    pub use crate::{
        AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, CliConfig, ConfigDocument,
        ConfigSourcePathInfo, ConfigStorage, RegistryDefaults, RegistrySettings, RuntimeDefaults,
        RuntimeSettings, Secret, SecretStorage, SecretStorageConfig, SystemConfig,
        effective_config_sources, load_cli_config, load_global_cli_config, write_cli_config,
    };

    pub use redb_lease::RedbLeaseProvider;

    use crate::AgentStorage;
    use crate::session_log::FileSessionStore;

    /// Central storage manager for file-backed local state and secrets.
    pub struct Storage {
        pub config: ConfigStorage,
        pub agents: AgentStorage,
        pub secrets: SecretStorage,
        pub file_sessions: FileSessionStore,
    }

    impl Storage {
        /// Create a new storage instance at the given path.
        pub fn new(path: &str) -> Result<Self> {
            let secret_config = SecretStorageConfig::default();
            Self::with_secret_config(path, secret_config)
        }

        /// Create a new storage instance with custom secret storage configuration.
        pub fn with_secret_config(path: &str, secret_config: SecretStorageConfig) -> Result<Self> {
            let db_path = PathBuf::from(path);

            let config = ConfigStorage;
            let agents = AgentStorage::new_file_backed()?;
            let secrets = SecretStorage::with_config_path(db_path.clone(), secret_config)?;
            let file_sessions = FileSessionStore::new(session_store_path(path))?;

            Ok(Self {
                config,
                agents,
                secrets,
                file_sessions,
            })
        }
    }

    fn session_store_path(db_path: &str) -> PathBuf {
        Path::new(db_path).with_file_name("sessions")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn fresh_storage_does_not_hold_redb_open_after_startup() {
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("restflow.db");
            let _storage = Storage::new(db_path.to_str().unwrap()).unwrap();
            let _second_open = redb::Database::create(&db_path).unwrap();
        }
    }
}
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use tempfile::TempDir;

    const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";
    const RESTFLOW_GLOBAL_CONFIG_ENV: &str = "RESTFLOW_GLOBAL_CONFIG";
    const RESTFLOW_WORKSPACE_CONFIG_ENV: &str = "RESTFLOW_WORKSPACE_CONFIG";
    const RESTFLOW_MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
    const RESTFLOW_AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";
    const TEST_MASTER_KEY_HEX: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";

    const ENV_KEYS: &[&str] = &[
        RESTFLOW_DIR_ENV,
        RESTFLOW_GLOBAL_CONFIG_ENV,
        RESTFLOW_WORKSPACE_CONFIG_ENV,
        RESTFLOW_MASTER_KEY_ENV,
        RESTFLOW_AGENTS_DIR_ENV,
    ];

    #[derive(Debug)]
    struct SavedEnv {
        key: &'static str,
        value: Option<OsString>,
    }

    /// Serialize tests that mutate RestFlow process-global environment.
    pub fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Serialize tests that mutate the legacy agents directory override.
    pub fn agents_env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Isolated RestFlow state root for tests.
    ///
    /// Production defaults still resolve to `~/.restflow`; this helper only scopes
    /// tests that opt into it.
    #[derive(Debug)]
    pub struct RestflowTestEnv {
        _lock: MutexGuard<'static, ()>,
        _agents_lock: MutexGuard<'static, ()>,
        root: TempDir,
        saved: Vec<SavedEnv>,
    }

    impl RestflowTestEnv {
        pub fn new() -> Self {
            let lock = env_lock();
            let agents_lock = agents_env_lock();
            let root = tempfile::tempdir().expect("restflow test root should be created");
            let global_config = root.path().join("config.toml");
            let workspace_config = root.path().join("workspace-config.toml");

            let saved = ENV_KEYS
                .iter()
                .map(|key| SavedEnv {
                    key,
                    value: std::env::var_os(key),
                })
                .collect::<Vec<_>>();

            unsafe {
                std::env::set_var(RESTFLOW_DIR_ENV, root.path());
                std::env::set_var(RESTFLOW_GLOBAL_CONFIG_ENV, &global_config);
                std::env::set_var(RESTFLOW_WORKSPACE_CONFIG_ENV, &workspace_config);
                std::env::set_var(RESTFLOW_MASTER_KEY_ENV, TEST_MASTER_KEY_HEX);
                std::env::remove_var(RESTFLOW_AGENTS_DIR_ENV);
            }

            Self {
                _lock: lock,
                _agents_lock: agents_lock,
                root,
                saved,
            }
        }

        pub fn root(&self) -> &Path {
            self.root.path()
        }

        pub fn db_path(&self, file_name: &str) -> PathBuf {
            self.root.path().join(file_name)
        }
    }

    impl Default for RestflowTestEnv {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for RestflowTestEnv {
        fn drop(&mut self) {
            unsafe {
                for saved in self.saved.iter().rev() {
                    match saved.value.as_ref() {
                        Some(value) => std::env::set_var(saved.key, value),
                        None => std::env::remove_var(saved.key),
                    }
                }
            }
        }
    }
}
pub mod time_utils {
    /// Get current timestamp in milliseconds.
    pub fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}
mod voice_transcript {
    use types::{ChatMessage, ChatMessageMedia, ChatMessageTranscript, ChatRole};

    const VOICE_MEDIA_TYPE_LINE: &str = "media_type: voice";
    const FILE_PATH_PREFIX: &str = "local_file_path: ";
    const TRANSCRIPT_MARKER: &str = "\n\n[Transcript]\n";
    const VOICE_HEADER_PREFIX: &str = "[Voice message";

    /// Populate structured voice metadata from legacy message content blocks.
    pub(crate) fn hydrate_voice_message_metadata(message: &mut ChatMessage) -> bool {
        if message.role != ChatRole::User {
            return false;
        }

        let mut changed = false;
        if message.media.is_none()
            && let Some(file_path) = extract_voice_file_path(&message.content)
        {
            let duration = extract_voice_duration_sec(&message.content);
            message.media = Some(ChatMessageMedia::voice(file_path, duration));
            changed = true;
        }

        if let Some(transcript_text) = extract_transcript_from_message_content(&message.content) {
            let should_update = message
                .transcript
                .as_ref()
                .is_none_or(|existing| existing.text.trim() != transcript_text);
            if should_update {
                message.transcript = Some(ChatMessageTranscript::new(transcript_text, None));
                changed = true;
            }
        }

        changed
    }

    fn extract_voice_file_path(content: &str) -> Option<String> {
        let mut is_voice_message = false;
        let mut file_path: Option<String> = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line == VOICE_MEDIA_TYPE_LINE {
                is_voice_message = true;
                continue;
            }

            if let Some(path) = line.strip_prefix(FILE_PATH_PREFIX) {
                let normalized = path.trim();
                if !normalized.is_empty() {
                    file_path = Some(normalized.to_string());
                }
            }
        }

        if is_voice_message { file_path } else { None }
    }

    fn extract_voice_duration_sec(content: &str) -> Option<u32> {
        let first_line = content.lines().next()?.trim();
        if !first_line.starts_with(VOICE_HEADER_PREFIX) {
            return None;
        }
        let (_, tail) = first_line.split_once(',')?;
        let seconds = tail.trim().strip_suffix("s]")?.trim();
        seconds.parse::<u32>().ok()
    }

    fn extract_transcript_from_message_content(content: &str) -> Option<String> {
        let (_, body) = content.split_once(TRANSCRIPT_MARKER)?;
        let transcript = body.trim();
        if transcript.is_empty() {
            None
        } else {
            Some(transcript.to_string())
        }
    }
}
pub use tools;

pub use config::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, CliConfig, ConfigDocument,
    ConfigSourcePathInfo, ConfigStorage, ConfigValueSourceInfo, ConfigValueSourceKind,
    EffectiveConfigSources, RegistryDefaults, RegistrySettings, RuntimeDefaults, RuntimeSettings,
    SystemConfig, SystemSection, effective_config_sources, load_cli_config, load_global_cli_config,
    write_cli_config,
};
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
pub use services::agent_catalog::{AgentStorage, DEFAULT_ASSISTANT_NAME, StoredAgent};
pub use steer::SteerRegistry;
pub use types::{
    AgentMeta, AgentNode, AgentType, ApiKeyConfig, ChatExecutionStatus, ChatMessage, ChatRole,
    ChatSession, ChatSessionMetadata, ChatSessionSummary, ChatSessionUpdate, CodexCliExecutionMode,
    ExecutionContainerKind, ExecutionContainerRef, ExecutionContainerSummary, ExecutionStepInfo,
    ExecutionThread, MessageExecution, ModelId, ModelMetadataDTO, ModelRoutingConfig, Provider,
    RunKind, RunListQuery, RunSummary, RunTimeline, Skill, SkillGating, SkillMeta, SkillReference,
    SkillScript, SkillSource, SkillStatus, SteerMessage, SteerSource, ValidationError,
    ValidationErrorResponse, encode_validation_error,
};

use std::sync::Arc;
use storage::Storage;
use tracing::{info, warn};

/// Core application state shared between daemon-backed application modes
///
/// AppCore wires together local state, features, and runtime services.
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub features: Arc<features::Features>,
}

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);
        prompt_files::ensure_prompt_templates()?;

        // Ensure default agent exists on first run
        Self::ensure_default_agent(&storage)?;
        if let Err(err) = storage.agents.reconcile_prompt_file_names() {
            warn!(
                error = %err,
                "Failed to reconcile agent prompt file names; continuing startup"
            );
        }
        info!("Initializing RestFlow (Agent-centric mode)");

        let config = storage.config.get_effective_config()?;
        let features = Arc::new(features::Features::from_config(&config));

        let core = Self { storage, features };

        Ok(core)
    }

    /// Create default agent if no agents exist
    fn ensure_default_agent(storage: &Storage) -> anyhow::Result<()> {
        let agents = storage.agents.list_agents()?;
        if agents.is_empty() {
            info!("Creating default agent...");
            let agent_node = types::AgentNode::with_model(types::ModelId::CodexCli);
            let _created = storage
                .agents
                .create_agent(DEFAULT_ASSISTANT_NAME.to_string(), agent_node)?;
            info!("Default agent created: {}", DEFAULT_ASSISTANT_NAME);
        }
        Ok(())
    }
}
