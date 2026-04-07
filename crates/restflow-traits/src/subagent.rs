//! Sub-agent data types and trait definitions.
//!
//! Runtime implementations (SubagentTracker, spawn_subagent) remain in restflow-ai.

use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::{
    DEFAULT_AGENT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_SUBAGENTS, DEFAULT_SUBAGENT_MAX_DEPTH,
    DEFAULT_SUBAGENT_TIMEOUT_SECS, TeamExecutionContext,
};
pub use restflow_contracts::request::RunSpawnRequest as ContractRunSpawnRequest;
/// Canonical contract request alias for child run spawning.
pub type ContractChildRunSpawnRequest = ContractRunSpawnRequest;
/// Legacy alias kept for compatibility with existing callers.
pub type ContractSubagentSpawnRequest = ContractRunSpawnRequest;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder").
    ///
    /// When omitted, runtime creates a temporary sub-agent from `inline` config.
    #[serde(default)]
    pub agent_id: Option<String>,

    /// Optional inline configuration for temporary child-run creation.
    ///
    /// This is used when `agent_id` is omitted.
    #[serde(default)]
    pub inline: Option<InlineRunConfig>,

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

    /// Optional parent run ID used for context propagation.
    ///
    /// The serialized field name is canonicalized to `parent_run_id` while
    /// still accepting legacy `parent_execution_id` input for compatibility.
    #[serde(default, rename = "parent_run_id", alias = "parent_execution_id")]
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

    /// Optional authoritative run ID for this sub-agent execution.
    ///
    /// When provided, runtime must use this as the canonical child run ID.
    #[serde(default)]
    pub run_id: Option<String>,

    /// Optional team execution context for teammate-managed child runs.
    #[serde(default)]
    pub team_context: Option<TeamExecutionContext>,
}

impl SpawnRequest {
    /// Returns the canonical parent run identifier for this child spawn.
    pub fn parent_run_id(&self) -> Option<&str> {
        self.parent_execution_id.as_deref()
    }

    /// Sets the canonical parent run identifier while preserving legacy storage.
    pub fn set_parent_run_id(&mut self, parent_run_id: Option<String>) {
        self.parent_execution_id = parent_run_id;
    }
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

/// Canonical inline run configuration alias.
pub type InlineRunConfig = InlineSubagentConfig;

/// Canonical child-run inline configuration alias.
pub type InlineChildRunConfig = InlineRunConfig;

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

    /// Parent run ID, when spawned from another execution.
    pub parent_run_id: Option<String>,

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

    /// Parent run ID, when this completion belongs to a child run.
    pub parent_run_id: Option<String>,

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
        request: ContractRunSpawnRequest,
    ) -> std::result::Result<SpawnHandle, ToolError>;

    /// List all callable sub-agent definitions.
    fn list_callable(&self) -> Vec<SubagentDefSummary>;

    /// List currently running sub-agents across all parents.
    ///
    /// This is the legacy/global view kept for backward compatibility.
    fn list_running(&self) -> Vec<SubagentState>;

    /// List currently running sub-agents that belong to one parent run.
    fn list_running_for_parent(&self, parent_run_id: &str) -> Vec<SubagentState> {
        let parent_run_id = parent_run_id.trim();
        if parent_run_id.is_empty() {
            return Vec::new();
        }

        self.list_running()
            .into_iter()
            .filter(|state| state.parent_run_id.as_deref() == Some(parent_run_id))
            .collect()
    }

    /// Number of currently running sub-agents.
    fn running_count(&self) -> usize;

    /// Wait for a sub-agent to complete, returning its terminal outcome.
    async fn wait(&self, task_id: &str) -> Option<SubagentCompletion>;

    /// Wait for a sub-agent that is owned by the given parent run.
    async fn wait_for_parent_owned_task(
        &self,
        task_id: &str,
        parent_run_id: &str,
    ) -> Option<SubagentCompletion>;

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
    use std::sync::Mutex;

    struct MockSubagentManager {
        running: Mutex<Vec<SubagentState>>,
        config: SubagentConfig,
    }

    #[async_trait::async_trait]
    impl SubagentManager for MockSubagentManager {
        fn spawn(
            &self,
            _request: ContractRunSpawnRequest,
        ) -> std::result::Result<SpawnHandle, ToolError> {
            Err(ToolError::Tool("not implemented".to_string()))
        }

        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            Vec::new()
        }

        fn list_running(&self) -> Vec<SubagentState> {
            self.running.lock().expect("running lock").clone()
        }

        fn running_count(&self) -> usize {
            self.running.lock().expect("running lock").len()
        }

        async fn wait(&self, _task_id: &str) -> Option<SubagentCompletion> {
            None
        }

        async fn wait_for_parent_owned_task(
            &self,
            _task_id: &str,
            _parent_run_id: &str,
        ) -> Option<SubagentCompletion> {
            None
        }

        fn config(&self) -> &SubagentConfig {
            &self.config
        }
    }

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

    #[test]
    fn test_list_running_for_parent_filters_legacy_global_view() {
        let manager = MockSubagentManager {
            running: Mutex::new(vec![
                SubagentState {
                    id: "run-1".to_string(),
                    agent_name: "child-a".to_string(),
                    task: "task-a".to_string(),
                    parent_run_id: Some("parent-1".to_string()),
                    status: SubagentStatus::Running,
                    started_at: 1,
                    completed_at: None,
                    result: None,
                },
                SubagentState {
                    id: "run-2".to_string(),
                    agent_name: "child-b".to_string(),
                    task: "task-b".to_string(),
                    parent_run_id: Some("parent-2".to_string()),
                    status: SubagentStatus::Running,
                    started_at: 2,
                    completed_at: None,
                    result: None,
                },
            ]),
            config: SubagentConfig::default(),
        };

        let filtered = manager.list_running_for_parent("parent-1");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "run-1");
    }

    #[test]
    fn test_list_running_for_parent_rejects_blank_parent() {
        let manager = MockSubagentManager {
            running: Mutex::new(Vec::new()),
            config: SubagentConfig::default(),
        };

        assert!(manager.list_running_for_parent("   ").is_empty());
    }

    #[test]
    fn test_spawn_request_serializes_parent_run_id_canonically() {
        let mut request = SpawnRequest {
            agent_id: Some("coder".to_string()),
            inline: None,
            task: "Investigate".to_string(),
            timeout_secs: None,
            max_iterations: None,
            priority: None,
            model: None,
            model_provider: None,
            parent_execution_id: None,
            trace_session_id: None,
            trace_scope_id: None,
            run_id: None,
            team_context: None,
        };
        request.set_parent_run_id(Some("parent-1".to_string()));

        let serialized = serde_json::to_value(request).expect("serialize spawn request");
        assert_eq!(serialized["parent_run_id"], "parent-1");
        assert!(serialized.get("parent_execution_id").is_none());
    }

    #[test]
    fn test_spawn_request_accepts_legacy_parent_execution_id_alias() {
        let request: SpawnRequest = serde_json::from_value(serde_json::json!({
            "task": "Investigate",
            "parent_execution_id": "legacy-parent"
        }))
        .expect("deserialize spawn request");

        assert_eq!(request.parent_run_id(), Some("legacy-parent"));
        assert_eq!(
            request.parent_execution_id.as_deref(),
            Some("legacy-parent")
        );
    }
}
