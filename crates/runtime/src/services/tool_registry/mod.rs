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
#[cfg(test)]
use crate::session_log::FileSessionStore;
use crate::storage::ConfigStorage;
#[cfg(test)]
use crate::storage::{AgentStorage, SecretStorage, TaskStorage};
use crate::{AgentDefaults, SystemConfig};
#[cfg(test)]
use ai::AgentState;
#[cfg(test)]
use ai::agent::{
    StreamEmitter, SubagentConfig, SubagentDefLookup, SubagentExecutionBridge, SubagentManagerImpl,
    SubagentTracker, execute_subagent_plan,
};
#[cfg(test)]
use ai::llm::{CodexClient, DefaultLlmClientFactory, LlmClient, LlmClientFactory};
#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::mpsc;
use tools::ToolRegistryBuilder;
use tracing::warn;
#[cfg(test)]
use types::LlmProvider;
use types::registry::ToolRegistry;
use types::security::SecurityGate;
#[cfg(test)]
use types::{ExecutionOutcome, ExecutionPlan};

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
