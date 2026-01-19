//! Agent module - ReAct loop implementation
//!
//! Implements the ReAct (Reasoning + Acting) pattern:
//! 1. Think - LLM reasons about the current state
//! 2. Decide - LLM chooses an action
//! 3. Act - Execute the chosen tool
//! 4. Observe - Record the result
//! 5. Repeat until goal is achieved or max iterations

mod executor;
mod state;
mod trace;

pub use executor::{AgentConfig, AgentExecutor, AgentResult};
pub use state::{AgentState, AgentStatus};
pub use trace::TraceEvent;
