//! Sub-agent spawning support for tool-based execution.

mod manager;
mod model_resolution;
mod spawn;
mod trace;
mod tracker;

pub use manager::{SubagentDeps, SubagentManagerImpl};
pub use restflow_trace::{RunTraceContext, RunTraceLifecycleSink, RunTraceOutcome};
pub use spawn::spawn_subagent;
pub use trace::{RunTraceEmitterFactory, RunTraceSink};
pub use tracker::SubagentTracker;

pub use restflow_traits::subagent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentResult, SubagentSpawner,
    SubagentState, SubagentStatus,
};
