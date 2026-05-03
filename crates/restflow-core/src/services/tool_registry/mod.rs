//! Tool registry service for creating tool registries with storage access.
//!
//! Adapter implementations live in [`super::adapters`]. This module provides
//! the [`create_tool_registry`] function that wires adapters into tools.

#[cfg(test)]
use crate::models::ModelId;
#[cfg(test)]
use crate::runtime::agent::main_agent_default_tool_names;
use crate::runtime::agent::tools::assembly::{
    register_bash_execution_tool, register_file_execution_tool,
};
#[cfg(test)]
use crate::runtime::orchestrator::{AgentOrchestratorImpl, ExecutionBackend};
#[cfg(test)]
use crate::runtime::subagent::StorageBackedSubagentLookup;
use crate::services::adapters::*;
use crate::storage::{
    AgentStorage, ChannelSessionBindingStorage, ChatSessionStorage, ConfigStorage,
    ExecutionTraceStorage, MemoryStorage, SecretStorage, TaskStorage, TerminalSessionStorage,
};
#[cfg(test)]
use restflow_ai::AgentState;
#[cfg(test)]
use restflow_ai::agent::{
    StreamEmitter, SubagentConfig, SubagentDefLookup, SubagentExecutionBridge, SubagentManagerImpl,
    SubagentTracker, execute_subagent_plan,
};
#[cfg(test)]
use restflow_ai::llm::{CodexClient, DefaultLlmClientFactory, LlmClient, LlmClientFactory};
#[cfg(test)]
use restflow_models::LlmProvider;
use restflow_storage::{AgentDefaults, SystemConfig};
use restflow_tools::ToolRegistryBuilder;
use restflow_traits::registry::ToolRegistry;
use restflow_traits::security::SecurityGate;
#[cfg(test)]
use restflow_traits::{ExecutionOutcome, ExecutionPlan};
#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::mpsc;
use tracing::warn;

const DEFAULT_SECURITY_AGENT_ID: &str = "unknown-agent";
const DEFAULT_SECURITY_TASK_ID: &str = "tool-registry";

mod assembly;
mod config;
#[cfg(test)]
mod subagent_backend;

use self::config::load_agent_defaults;
#[cfg(test)]
use self::config::{build_llm_factory, load_subagent_config};

pub use self::assembly::{create_tool_registry, create_tool_registry_with_assessor};

#[cfg(test)]
mod tests;
