//! Agent module - Pluggable execution strategies
//!
//! ## Default Strategy: ReAct (Reasoning + Acting)
//!
//! 1. Think - LLM reasons about the current state
//! 2. Decide - LLM chooses an action
//! 3. Act - Execute the chosen tool
//! 4. Observe - Record the result
//! 5. Repeat until goal is achieved or max iterations
//!
//! ## Available Strategies
//!
//! | Strategy | Status | Best For |
//! |----------|--------|----------|
//! | ReAct | ✅ Implemented | General tasks |
//! | Pre-Act | 🚧 Planned | Cost optimization |
//! | Reflexion | 🚧 Planned | Learning from failures |
//! | Hierarchical | 🚧 Planned | Complex multi-part tasks |
//! | Swarm | 🚧 Planned | Multi-agent collaboration |
//! | Tree-of-Thought | 🚧 Planned | Creative problem solving |
//!
//! ## Usage
//!
//! ```rust,ignore
//! use restflow_ai::agent::strategy::{AgentStrategyFactory, StrategyType};
//!
//! // Use default (ReAct)
//! let agent = AgentStrategyFactory::default(llm, tools);
//! let result = agent.execute(config).await?;
//!
//! // Use specific strategy
//! let agent = AgentStrategyFactory::create(StrategyType::PreAct, llm, tools);
//! let result = agent.execute(config).await?;
//! ```

mod context;
mod executor;
pub mod react;
mod state;
pub mod strategy;
mod trace;
mod unified;

pub use context::{
    AgentContext, MemoryContext, SkillSummary, get_project_context, load_workspace_context,
};
pub use executor::{AgentConfig, AgentExecutor, AgentResult, AgentType};
pub use state::{AgentState, AgentStatus};
pub use trace::TraceEvent;
pub use unified::{ExecutionResult, UnifiedAgent, UnifiedAgentConfig};
