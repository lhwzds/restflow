//! Sub-agent data types and trait definitions.
//!
//! Runtime implementations (SubagentTracker, spawn_subagent) remain in restflow-ai.

use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::{
    DEFAULT_AGENT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_SUBAGENTS, DEFAULT_SUBAGENT_MAX_DEPTH,
    DEFAULT_SUBAGENT_TIMEOUT_SECS,
};
pub use restflow_contracts::request::SubagentSpawnRequest as ContractSubagentSpawnRequest;

/// Snapshot of a sub-agent definition with all fields needed for execution.
///
/// This is a simple owned data struct that captures the fields from a concrete
/// agent definition. It decouples the restflow-ai crate from the full
/// `AgentDefinition` struct (which lives in restflow-core and carries
/// `#[derive(TS)]` and other derives that restflow-ai doesn't need).
#[derive(Debug, Clone)]
pub struct SubagentDefSnapshot {
    /// Display name
    pub name: String,
    /// System prompt for the agent
    pub system_prompt: String,
    /// Allowed tool names
    pub allowed_tools: Vec<String>,
    /// Maximum ReAct loop iterations
    pub max_iterations: Option<u32>,
    /// Default model for this agent type (from agent definition).
    pub default_model: Option<String>,
}

/// Summary info for listing a sub-agent definition.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubagentDefSummary {
    /// Unique identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description of when to use this agent
    pub description: String,
    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Trait for looking up sub-agent definitions by ID.
///
/// Implemented by `AgentDefinitionRegistry` in restflow-core so that
/// restflow-ai can spawn sub-agents without depending on restflow-core.
pub trait SubagentDefLookup: Send + Sync {
    /// Look up a sub-agent definition by ID, returning a snapshot of the
    /// fields needed for execution.
    fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot>;

    /// List all callable sub-agent definitions (for display/listing purposes).
    fn list_callable(&self) -> Vec<SubagentDefSummary>;
}

/// Configuration for sub-agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Maximum number of parallel sub-agents.
    pub max_parallel_agents: usize,
    /// Default timeout for sub-agents in seconds.
    pub subagent_timeout_secs: u64,
    /// Maximum iterations for sub-agents.
    pub max_iterations: usize,
    /// Maximum nesting depth for sub-agents.
    pub max_depth: usize,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: DEFAULT_MAX_PARALLEL_SUBAGENTS,
            subagent_timeout_secs: DEFAULT_SUBAGENT_TIMEOUT_SECS,
            max_iterations: DEFAULT_AGENT_MAX_ITERATIONS,
            max_depth: DEFAULT_SUBAGENT_MAX_DEPTH,
        }
    }
}

/// Request to spawn a sub-agent.
#[derive(Debug, Clone, Serialize)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder").
    ///
    /// When omitted, runtime creates a temporary sub-agent from `inline` config.
    #[serde(default)]
    pub agent_id: Option<String>,

    /// Optional inline configuration for temporary sub-agent creation.
    ///
    /// This is used when `agent_id` is omitted.
    #[serde(default)]
    pub inline: Option<InlineSubagentConfig>,

    /// Task description for the agent.
    pub task: String,

    /// Optional timeout in seconds.
    pub timeout_secs: Option<u64>,

    /// Optional max iterations override for this spawn.
    #[serde(default)]
    pub max_iterations: Option<u32>,

    /// Optional priority level.
    pub priority: Option<SpawnPriority>,

    /// Optional model override for this spawn (e.g., "minimax/coding-plan").
    #[serde(default)]
    pub model: Option<String>,

    /// Optional provider selector paired with `model` (e.g., "openai-codex").
    ///
    /// When provided, runtime validates that the resolved model belongs to this provider.
    #[serde(default)]
    pub model_provider: Option<String>,

    /// Optional parent execution ID used for context propagation.
    ///
    /// This is injected by runtime when sub-agents are spawned from another
    /// agent execution loop.
    #[serde(default)]
    pub parent_execution_id: Option<String>,

    /// Optional trace session identifier used to keep child runs in the same trace session.
    ///
    /// This is injected by runtime and should not be supplied by users directly.
    #[serde(default)]
    pub trace_session_id: Option<String>,

    /// Optional trace scope identifier used to group execution events for this child run.
    ///
    /// This is injected by runtime and should not be supplied by users directly.
    #[serde(default)]
    pub trace_scope_id: Option<String>,
}

/// Inline configuration for temporary sub-agent creation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InlineSubagentConfig {
    /// Display name for the temporary sub-agent.
    #[serde(default)]
    pub name: Option<String>,

    /// System prompt override for the temporary sub-agent.
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Allowed tool names for the temporary sub-agent.
    ///
    /// If omitted, runtime uses all tools currently available to the parent.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Optional max iterations override for the temporary sub-agent.
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

/// Priority level for sub-agent spawning.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Source used to determine one effective sub-agent limit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentLimitSource {
    ConfigDefault,
    RequestOverride,
    InlineConfig,
    AgentDefinition,
}

/// Effective sub-agent runtime limits resolved at spawn time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentEffectiveLimits {
    /// Effective timeout in seconds.
    pub timeout_secs: u64,
    /// Where the timeout value came from.
    pub timeout_source: SubagentLimitSource,
    /// Effective maximum iterations.
    pub max_iterations: usize,
    /// Where the max_iterations value came from.
    pub max_iterations_source: SubagentLimitSource,
}

/// Handle returned after spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnHandle {
    /// Unique task ID.
    pub id: String,

    /// Agent name.
    pub agent_name: String,

    /// Effective runtime limits resolved for this spawn.
    pub effective_limits: SubagentEffectiveLimits,
}

/// Sub-agent running state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SubagentState {
    /// Unique task ID
    pub id: String,

    /// Agent name (e.g., "researcher", "coder")
    pub agent_name: String,

    /// Task description
    pub task: String,

    /// Current status
    pub status: SubagentStatus,

    /// Start timestamp (Unix ms)
    pub started_at: i64,

    /// Completion timestamp (Unix ms)
    pub completed_at: Option<i64>,

    /// Result (when completed)
    pub result: Option<SubagentResult>,
}

/// Sub-agent status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Interrupted,
    TimedOut,
}

/// Result from a sub-agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SubagentResult {
    /// Whether execution succeeded
    pub success: bool,

    /// Output content
    pub output: String,

    /// Optional summary of the output
    pub summary: Option<String>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Tokens used
    pub tokens_used: Option<u32>,

    /// Cost in USD
    pub cost_usd: Option<f64>,

    /// Error message (if failed)
    pub error: Option<String>,
}

/// Completion notification
#[derive(Debug, Clone)]
pub struct SubagentCompletion {
    /// Task ID
    pub id: String,

    /// Final terminal status.
    pub status: SubagentStatus,

    /// Execution result payload when available.
    pub result: Option<SubagentResult>,
}

/// High-level subagent lifecycle management.
///
/// Abstracts `SubagentTracker` + `SubagentDefLookup` + `spawn_subagent` so that
/// tool implementations can manage subagents without depending on `restflow-ai`.
#[async_trait::async_trait]
pub trait SubagentManager: Send + Sync {
    /// Spawn a new sub-agent from a contract request payload.
    fn spawn(
        &self,
        request: ContractSubagentSpawnRequest,
    ) -> std::result::Result<SpawnHandle, ToolError>;

    /// List all callable sub-agent definitions.
    fn list_callable(&self) -> Vec<SubagentDefSummary>;

    /// List currently running sub-agents.
    fn list_running(&self) -> Vec<SubagentState>;

    /// Number of currently running sub-agents.
    fn running_count(&self) -> usize;

    /// Wait for a sub-agent to complete, returning its terminal outcome.
    async fn wait(&self, task_id: &str) -> Option<SubagentCompletion>;

    /// Access the sub-agent configuration.
    fn config(&self) -> &SubagentConfig;
}

/// Trait for spawning subagents (simple variant used by SpawnTool).
pub trait SubagentSpawner: Send + Sync {
    fn spawn(&self, task: String) -> std::result::Result<String, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_handle_serialization() {
        let handle = SpawnHandle {
            id: "task-123".to_string(),
            agent_name: "Researcher".to_string(),
            effective_limits: SubagentEffectiveLimits {
                timeout_secs: 300,
                timeout_source: SubagentLimitSource::ConfigDefault,
                max_iterations: 100,
                max_iterations_source: SubagentLimitSource::ConfigDefault,
            },
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("task-123"));
    }
}
