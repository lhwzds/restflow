//! Core traits for agent strategies
//!
//! This module defines the unified interface that all agent strategies must implement.
//!
//! # Design Notes
//!
//! The trait uses `async fn execute() -> Result<StrategyResult>` instead of
//! streaming/event-driven APIs because:
//!
//! - Simpler to implement new strategies
//! - Easier to test (check input/output)
//! - Strategies can compose/chain easily
//! - Streaming can be added as optional layer later
//!
//! See `mod.rs` for full architecture comparison with event-driven systems.

use std::collections::HashMap;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unified configuration for all strategies
#[derive(Debug, Clone)]
pub struct StrategyConfig {
    /// The goal/task for the agent to accomplish
    pub goal: String,

    /// Optional system prompt override
    pub system_prompt: Option<String>,

    /// Maximum iterations/steps allowed
    pub max_iterations: usize,

    /// Timeout for tool execution
    pub tool_timeout: Duration,

    /// Hidden context passed to tools (not shown to LLM)
    pub context: HashMap<String, Value>,

    /// Strategy-specific options
    pub options: StrategyOptions,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            goal: String::new(),
            system_prompt: None,
            max_iterations: 10,
            tool_timeout: Duration::from_secs(30),
            context: HashMap::new(),
            options: StrategyOptions::default(),
        }
    }
}

impl StrategyConfig {
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            ..Default::default()
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_options(mut self, options: StrategyOptions) -> Self {
        self.options = options;
        self
    }
}

/// Strategy-specific options
#[derive(Debug, Clone, Default)]
pub struct StrategyOptions {
    // Pre-Act options
    /// Use separate models for planning vs execution (Pre-Act)
    pub use_planner_executor_split: bool,
    /// Planner model (stronger, e.g., "claude-opus")
    pub planner_model: Option<String>,
    /// Executor model (cheaper, e.g., "claude-haiku")
    pub executor_model: Option<String>,

    // Reflexion options
    /// Enable learning from past failures
    pub enable_reflection: bool,
    /// Maximum reflections to include in context
    pub max_reflections: usize,

    // Hierarchical options
    /// Number of executor agents in pool
    pub executor_pool_size: usize,

    // Swarm options
    /// Maximum agents in swarm
    pub max_swarm_agents: usize,
    /// Swarm communication pattern
    pub swarm_pattern: SwarmPattern,

    // Tree-of-Thought options
    /// Branching factor (candidates per step)
    pub branching_factor: usize,
    /// Maximum tree depth
    pub max_depth: usize,
    /// Minimum score threshold for pruning
    pub prune_threshold: f32,
}

/// Swarm communication patterns
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SwarmPattern {
    /// Peer-to-peer: all agents can communicate directly
    #[default]
    Mesh,
    /// Hierarchical: queen coordinates workers
    Hierarchical,
    /// Broadcast: messages go to all agents
    Broadcast,
}

/// Result from strategy execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyResult {
    /// Whether the task was completed successfully
    pub success: bool,

    /// Final output/answer
    pub output: String,

    /// Number of iterations/steps taken
    pub iterations: usize,

    /// Total tokens used
    pub total_tokens: u32,

    /// Strategy-specific metadata
    pub strategy_metadata: StrategyMetadata,
}

/// Strategy-specific execution metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyMetadata {
    /// For Pre-Act: the generated plan
    pub plan: Option<Vec<String>>,

    /// For Reflexion: reflections generated
    pub reflections: Option<Vec<String>>,

    /// For Hierarchical: subtasks delegated
    pub subtasks: Option<Vec<SubtaskInfo>>,

    /// For Swarm: participating agents
    pub swarm_agents: Option<Vec<String>>,

    /// For ToT: paths explored
    pub paths_explored: Option<usize>,

    /// For ToT: best path taken
    pub best_path: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskInfo {
    pub id: String,
    pub description: String,
    pub assigned_to: String,
    pub status: String,
}

/// Core trait that all agent strategies must implement
#[async_trait::async_trait]
pub trait AgentStrategy: Send + Sync {
    /// Returns the strategy name
    fn name(&self) -> &'static str;

    /// Returns a brief description of how this strategy works
    fn description(&self) -> &'static str;

    /// Execute the strategy with the given configuration
    async fn execute(&self, config: StrategyConfig) -> crate::error::Result<StrategyResult>;

    /// Check if this strategy supports a specific feature
    fn supports_feature(&self, feature: StrategyFeature) -> bool {
        // Default: only basic features
        matches!(feature, StrategyFeature::BasicExecution)
    }

    /// Get recommended settings for this strategy
    fn recommended_settings(&self) -> RecommendedSettings {
        RecommendedSettings::default()
    }
}

/// Features that strategies may support
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyFeature {
    /// Basic execution (all strategies support this)
    BasicExecution,
    /// Parallel tool execution
    ParallelTools,
    /// Learning from failures
    Reflection,
    /// Multi-step planning
    Planning,
    /// Multi-agent collaboration
    MultiAgent,
    /// Streaming output
    Streaming,
    /// Checkpointing/resumption
    Checkpointing,
}

/// Recommended settings for a strategy
#[derive(Debug, Clone)]
pub struct RecommendedSettings {
    pub min_iterations: usize,
    pub max_iterations: usize,
    pub recommended_model: &'static str,
    pub estimated_cost_multiplier: f32,
    pub best_for: Vec<&'static str>,
}

impl Default for RecommendedSettings {
    fn default() -> Self {
        Self {
            min_iterations: 1,
            max_iterations: 10,
            recommended_model: "claude-sonnet",
            estimated_cost_multiplier: 1.0,
            best_for: vec!["general tasks"],
        }
    }
}
