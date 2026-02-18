//! Sub-agent spawning support for tool-based execution.

use super::definition::{AgentDefinition, AgentDefinitionRegistry};
use super::tracker::{SubagentResult, SubagentTracker};
use anyhow::{Result, anyhow};
use restflow_ai::llm::CompletionRequest;
use restflow_ai::{LlmClient, Message};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};
use ts_rs::TS;

/// Configuration for sub-agent execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentConfig {
    /// Maximum number of parallel sub-agents.
    pub max_parallel_agents: usize,
    /// Default timeout for sub-agents in seconds.
    pub subagent_timeout_secs: u64,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: 5,
            subagent_timeout_secs: 300,
        }
    }
}

/// Request to spawn a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder").
    pub agent_id: String,

    /// Task description for the agent.
    pub task: String,

    /// Optional timeout in seconds.
    pub timeout_secs: Option<u64>,

    /// Optional priority level.
    pub priority: Option<SpawnPriority>,

    /// Parent subflow path for hierarchical tracking.
    /// When set, the spawned agent will inherit this path and append its task ID.
    #[serde(default)]
    pub parent_subflow_path: Vec<String>,
}

/// Priority level for sub-agent spawning.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}
