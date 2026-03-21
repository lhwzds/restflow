//! Sub-agent spawning support for tool-based execution.

mod manager;
mod model_resolution;
mod spawn;
mod tracker;

pub use manager::{SubagentDeps, SubagentManagerImpl};
pub use restflow_trace::RunTraceContext;
pub use spawn::{SubagentExecutionBridge, execute_subagent_once, spawn_subagent};
pub use tracker::SubagentTracker;

pub use restflow_traits::subagent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentResult, SubagentSpawner,
    SubagentState, SubagentStatus,
};
