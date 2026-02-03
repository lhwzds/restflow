//! Agent module - ReAct loop implementation
//!
//! Implements the ReAct (Reasoning + Acting) pattern:
//! 1. Think - LLM reasons about the current state
//! 2. Decide - LLM chooses an action
//! 3. Act - Execute the chosen tool
//! 4. Observe - Record the result
//! 5. Repeat until goal is achieved or max iterations

mod context;
mod executor;
pub mod react;
mod state;
mod trace;
mod unified;

pub use context::{load_workspace_context, AgentContext, MemoryContext, SkillSummary};
pub use executor::{AgentConfig, AgentExecutor, AgentResult};
pub use state::{AgentState, AgentStatus};
pub use trace::TraceEvent;
pub use unified::{ExecutionResult, UnifiedAgent, UnifiedAgentConfig};
