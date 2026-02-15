//! Agent Strategy Module - Pluggable execution strategies
//!
//! This module provides a unified interface for different agent execution strategies.
//! Each strategy implements the `AgentStrategy` trait, allowing seamless swapping.
//!
//! # Architecture Decision: ReAct Loop vs Event-Driven
//!
//! RestFlow uses a **ReAct loop** architecture. This section documents why and
//! compares it with event-driven systems like OpenAI Codex CLI.
//!
//! ## ReAct Loop (RestFlow's Current Approach)
//!
//! ```text
//! loop {
//!     response = llm.complete(prompt).await;  // Wait for full response
//!     if has_tool_call(response) {
//!         result = execute_tool(call).await;  // Sequential execution
//!         prompt.push(result);
//!     } else {
//!         return response;  // Done
//!     }
//! }
//! ```
//!
//! **Pros:**
//! - Simple (~200 LOC core)
//! - Easy to debug (sequential flow)
//! - Easy to implement new strategies
//! - Great for research and iteration
//!
//! **Cons:**
//! - No real-time streaming to UI
//! - Sequential tool execution
//! - Coarse-grained cancellation
//!
//! ## Event-Driven (Codex CLI's Approach)
//!
//! ```text
//! stream = llm.stream(prompt).await;
//! loop {
//!     match stream.next().await {
//!         TextDelta(text) => ui.append(text),        // Real-time
//!         ToolCall(call) => futures.push(exec(call)); // Parallel
//!         Completed => break,
//!     }
//! }
//! results = futures.collect().await;
//! ```
//!
//! **Pros:**
//! - Real-time streaming output
//! - Parallel tool execution
//! - Fine-grained cancellation
//! - Better production UX
//!
//! **Cons:**
//! - Complex (~2000 LOC core)
//! - Harder to debug (async events)
//! - More boilerplate for new strategies
//!
//! ## Comparison Table
//!
//! | Aspect | ReAct Loop | Event-Driven |
//! |--------|-----------|--------------|
//! | User experience | Wait for response | Real-time streaming |
//! | Tool execution | Sequential | Parallel possible |
//! | Cancellation | Wait for step | Immediate |
//! | Code complexity | Low | High |
//! | Debugging | Easy | Complex |
//! | Best for | Research | Production |
//!
//! ## Evolution Path
//!
//! 1. **Short term**: Keep ReAct simple, implement Pre-Act/ToT/etc
//! 2. **Mid term**: Add optional streaming output layer
//! 3. **Long term**: Consider event-driven if parallel tools critical
//!
//! # Available Strategies
//!
//! | Strategy | Status | Description |
//! |----------|--------|-------------|
//! | ReAct | ✅ Implemented | Reasoning + Acting loop |
//! | Pre-Act | 🚧 Planned | Plan first, then execute |
//! | Reflexion | 🚧 Planned | Self-reflection on failures |
//! | Hierarchical | 🚧 Planned | Planner + Executors |
//! | Swarm | 🚧 Planned | Multi-agent collaboration |
//! | TreeOfThought | 🚧 Planned | Multi-path exploration |
//!
//! # Usage
//!
//! ```rust,ignore
//! use restflow_ai::agent::strategy::{StrategyType, AgentStrategyFactory};
//!
//! // Create agent with specific strategy
//! let agent = AgentStrategyFactory::create(
//!     StrategyType::PreAct,
//!     llm_client,
//!     tools,
//! );
//!
//! // Or use default (ReAct)
//! let agent = AgentStrategyFactory::default(llm_client, tools);
//!
//! // Execute
//! let result = agent.execute(config).await?;
//! ```

mod code_first;
mod hierarchical;
mod preact;
mod reflexion;
mod swarm;
mod tot;
mod traits;

pub use code_first::{CodeFirstConfig, CodeFirstStrategy};
pub use hierarchical::{HierarchicalConfig, HierarchicalStrategy};
pub use preact::{PreActConfig, PreActStrategy};
pub use reflexion::{ReflexionConfig, ReflexionStrategy};
pub use swarm::{SwarmConfig, SwarmStrategy};
pub use tot::{TreeOfThoughtConfig, TreeOfThoughtStrategy};
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
    /// Pre-Act: Generate plan first, then execute steps
    PreAct,
    /// Reflexion: Learn from failures via self-reflection
    Reflexion,
    /// Hierarchical: Global planner + local executors
    Hierarchical,
    /// Swarm: Multi-agent collaboration without central orchestrator
    Swarm,
    /// Tree-of-Thought: Explore multiple reasoning paths
    TreeOfThought,
    /// CodeFirst: LLM generates Python code that calls tools as functions
    CodeFirst,
}

impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReAct => write!(f, "react"),
            Self::PreAct => write!(f, "preact"),
            Self::Reflexion => write!(f, "reflexion"),
            Self::Hierarchical => write!(f, "hierarchical"),
            Self::Swarm => write!(f, "swarm"),
            Self::TreeOfThought => write!(f, "tree-of-thought"),
            Self::CodeFirst => write!(f, "code-first"),
        }
    }
}

impl std::str::FromStr for StrategyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "react" => Ok(Self::ReAct),
            "preact" | "pre-act" => Ok(Self::PreAct),
            "reflexion" => Ok(Self::Reflexion),
            "hierarchical" => Ok(Self::Hierarchical),
            "swarm" => Ok(Self::Swarm),
            "tot" | "tree-of-thought" | "treeofthought" => Ok(Self::TreeOfThought),
            "codefirst" | "code-first" => Ok(Self::CodeFirst),
            _ => Err(format!("Unknown strategy: {}", s)),
        }
    }
}

/// Factory for creating agent strategies
pub struct AgentStrategyFactory;

impl AgentStrategyFactory {
    /// Create an agent with the specified strategy
    ///
    /// # Arguments
    /// * `strategy_type` - The type of strategy to use
    /// * `llm` - LLM client for reasoning
    /// * `tools` - Tool registry for actions
    ///
    /// # Returns
    /// A boxed strategy that implements `AgentStrategy`
    pub fn create(
        strategy_type: StrategyType,
        llm: Arc<dyn LlmClient>,
        tools: Arc<ToolRegistry>,
    ) -> Box<dyn AgentStrategy> {
        match strategy_type {
            StrategyType::ReAct => {
                // Use existing AgentExecutor wrapped in a strategy adapter
                Box::new(ReactStrategyAdapter::new(llm, tools))
            }
            StrategyType::PreAct => Box::new(PreActStrategy::new(llm, tools)),
            StrategyType::Reflexion => Box::new(ReflexionStrategy::new(llm, tools)),
            StrategyType::Hierarchical => Box::new(HierarchicalStrategy::new(llm, tools)),
            StrategyType::Swarm => Box::new(SwarmStrategy::new(llm, tools)),
            StrategyType::TreeOfThought => Box::new(TreeOfThoughtStrategy::new(llm, tools)),
            StrategyType::CodeFirst => Box::new(CodeFirstStrategy::new(llm, tools)),
        }
    }

    /// Create an agent with the default strategy (ReAct)
    pub fn default(llm: Arc<dyn LlmClient>, tools: Arc<ToolRegistry>) -> Box<dyn AgentStrategy> {
        Self::create(StrategyType::ReAct, llm, tools)
    }

    /// Check if a strategy is implemented (not just a placeholder)
    pub fn is_implemented(strategy_type: StrategyType) -> bool {
        matches!(
            strategy_type,
            StrategyType::ReAct | StrategyType::CodeFirst
        )
    }
}

/// Adapter to wrap existing AgentExecutor as an AgentStrategy.
///
/// Uses a simple ReAct loop (see module docs for architecture comparison).
/// This approach is intentionally simple to allow rapid experimentation
/// with different strategies without the complexity of event-driven systems.
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
