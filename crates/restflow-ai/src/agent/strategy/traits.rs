//! Core traits for agent strategies

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

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
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            goal: String::new(),
            system_prompt: None,
            max_iterations: 100,
            tool_timeout: Duration::from_secs(300),
            context: HashMap::new(),
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
}
