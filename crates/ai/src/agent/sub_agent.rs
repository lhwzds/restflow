//! Sub-agent spawning support for tool-based execution.

mod manager;
mod model_resolution;
mod spawn;
mod tracker;

pub use crate::telemetry::RunTraceContext;
pub use manager::{SubagentDeps, SubagentManagerImpl};
pub use spawn::{SubagentExecutionBridge, execute_subagent_plan};
pub use tracker::SubagentTracker;

pub use types::subagent::{
    SpawnHandle, SpawnPriority, SubagentCompletion, SubagentConfig, SubagentDefLookup,
    SubagentDefSnapshot, SubagentDefSummary, SubagentResult, SubagentSpawner, SubagentState,
    SubagentStatus,
};
