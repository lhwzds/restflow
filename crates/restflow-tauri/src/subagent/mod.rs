//! Sub-agent definitions and tracking.
//!
//! All runtime implementations come from restflow-ai. Definitions come from restflow-core.

pub use restflow_core::runtime::subagent::{
    AgentDefinition, AgentDefinitionRegistry, builtin_agents,
};
pub use restflow_ai::agent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig, SubagentResult,
    SubagentState, SubagentStatus, SubagentTracker, spawn_subagent,
};
