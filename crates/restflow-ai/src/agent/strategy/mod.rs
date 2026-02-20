//! Agent Strategy Module - Pluggable execution strategies
//!
//! This module provides a unified interface for different agent execution strategies.
//! Each strategy implements the `AgentStrategy` trait, allowing seamless swapping.
//!
//! # Available Strategies
//!
//! | Strategy | Status | Description |
//! |----------|--------|-------------|
//! | ReAct | ✅ Implemented | Reasoning + Acting loop |

mod traits;

pub use traits::{AgentStrategy, StrategyConfig, StrategyResult};

use crate::agent::AgentExecutor;
use crate::llm::LlmClient;
use crate::tools::ToolRegistry;
use std::sync::Arc;

/// Available agent strategy types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyType {
    /// ReAct: Reasoning + Acting loop (default, already implemented)
    #[default]
    ReAct,
}

impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReAct => write!(f, "react"),
        }
    }
}

impl std::str::FromStr for StrategyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "react" => Ok(Self::ReAct),
            _ => Err(format!("Unknown strategy: {}", s)),
        }
    }
}

/// Factory for creating agent strategies
pub struct AgentStrategyFactory;

impl AgentStrategyFactory {
    /// Create an agent with the specified strategy
    pub fn create(
        strategy_type: StrategyType,
        llm: Arc<dyn LlmClient>,
        tools: Arc<ToolRegistry>,
    ) -> Box<dyn AgentStrategy> {
        match strategy_type {
            StrategyType::ReAct => Box::new(ReactStrategyAdapter::new(llm, tools)),
        }
    }

    /// Create an agent with the default strategy (ReAct)
    pub fn default(llm: Arc<dyn LlmClient>, tools: Arc<ToolRegistry>) -> Box<dyn AgentStrategy> {
        Self::create(StrategyType::ReAct, llm, tools)
    }
}

/// Adapter to wrap existing AgentExecutor as an AgentStrategy.
struct ReactStrategyAdapter {
    llm: Arc<dyn LlmClient>,
    tools: Arc<ToolRegistry>,
}

impl ReactStrategyAdapter {
    fn new(llm: Arc<dyn LlmClient>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, tools }
    }
}

#[async_trait::async_trait]
impl AgentStrategy for ReactStrategyAdapter {
    fn name(&self) -> &'static str {
        "ReAct"
    }

    fn description(&self) -> &'static str {
        "Reasoning + Acting loop: Think → Act → Observe → Repeat"
    }

    async fn execute(&self, config: StrategyConfig) -> crate::error::Result<StrategyResult> {
        let executor = AgentExecutor::new(self.llm.clone(), self.tools.clone());

        let agent_config = crate::agent::AgentConfig::new(&config.goal)
            .with_max_iterations(config.max_iterations)
            .with_tool_timeout(config.tool_timeout);

        let result = executor.run(agent_config).await?;

        Ok(StrategyResult {
            success: result.success,
            output: result.answer.unwrap_or_default(),
            iterations: result.iterations,
            total_tokens: result.total_tokens,
            strategy_metadata: Default::default(),
        })
    }
}
