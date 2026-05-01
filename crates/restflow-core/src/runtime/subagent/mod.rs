//! Storage-backed sub-agent definition adapters.
//!
//! This module is intentionally limited to definition lookup and registry
//! plumbing. Runtime execution primitives such as `SubagentTracker`,
//! `SubagentManagerImpl`, and `spawn_subagent` are owned by `restflow-ai`.

pub mod definition;

pub use definition::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
