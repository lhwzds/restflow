//! Sub-agent definitions and tracking.

pub mod definition;
pub mod spawn;
pub mod tracker;

pub use definition::{AgentDefinition, AgentDefinitionRegistry, builtin_agents};
pub use spawn::spawn_subagent;
pub use tracker::{
    SubagentCompletion, SubagentResult, SubagentState, SubagentStatus, SubagentTracker,
};
// Canonical sub-agent types re-exported from restflow-traits via spawn module
pub use spawn::{SpawnHandle, SpawnPriority, SpawnRequest, SubagentConfig};
