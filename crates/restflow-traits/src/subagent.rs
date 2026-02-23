//! Sub-agent data types and trait definitions.
//!
//! Runtime implementations (SubagentTracker, spawn_subagent) remain in restflow-ai.

use serde::{Deserialize, Serialize};

use crate::error::ToolError;

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
            max_parallel_agents: 5,
            subagent_timeout_secs: 600,
            max_iterations: 20,
            max_depth: 1,
        }
    }
}

/// Request to spawn a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder").
    pub agent_id: String,

    /// Task description for the agent.
    pub task: String,

    /// Optional timeout in seconds.
    pub timeout_secs: Option<u64>,

    /// Optional priority level.
    pub priority: Option<SpawnPriority>,
}

/// Priority level for sub-agent spawning.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Handle returned after spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnHandle {
    /// Unique task ID.
    pub id: String,

    /// Agent name.
    pub agent_name: String,
}

/// Sub-agent running state
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub enum SubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

/// Result from a sub-agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Execution result
    pub result: SubagentResult,
}

/// High-level subagent lifecycle management.
///
/// Abstracts `SubagentTracker` + `SubagentDefLookup` + `spawn_subagent` so that
/// tool implementations can manage subagents without depending on `restflow-ai`.
#[async_trait::async_trait]
pub trait SubagentManager: Send + Sync {
    /// Spawn a new sub-agent from a [`SpawnRequest`].
    fn spawn(&self, request: SpawnRequest) -> std::result::Result<SpawnHandle, ToolError>;

    /// List all callable sub-agent definitions.
    fn list_callable(&self) -> Vec<SubagentDefSummary>;

    /// List currently running sub-agents.
    fn list_running(&self) -> Vec<SubagentState>;

    /// Number of currently running sub-agents.
    fn running_count(&self) -> usize;

    /// Wait for a sub-agent to complete, returning its result.
    async fn wait(&self, task_id: &str) -> Option<SubagentResult>;

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
    fn test_spawn_request_serialization() {
        let request = SpawnRequest {
            agent_id: "researcher".to_string(),
            task: "Research topic X".to_string(),
            timeout_secs: Some(300),
            priority: Some(SpawnPriority::High),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("researcher"));

        let parsed: SpawnRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "researcher");
    }

    #[test]
    fn test_spawn_handle_serialization() {
        let handle = SpawnHandle {
            id: "task-123".to_string(),
            agent_name: "Researcher".to_string(),
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("task-123"));
    }
}
