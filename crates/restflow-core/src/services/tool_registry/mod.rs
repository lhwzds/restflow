//! Tool registry service for creating tool registries with storage access.
//!
//! Adapter implementations live in [`super::adapters`]. This module provides
//! the [`create_tool_registry`] function that wires adapters into tools.

use crate::memory::UnifiedSearchEngine;
use crate::models::{ModelId, Provider};
use crate::process::ProcessRegistry;
use crate::runtime::agent::main_agent_default_tool_names;
use crate::runtime::agent::tools::assembly::{
    KNOWN_TOOL_ALIASES, populate_known_tools_from_registry, register_bash_execution_tool,
    register_file_execution_tool, register_http_execution_tool, register_python_execution_tools,
    register_send_email_execution_tool,
};
use crate::runtime::orchestrator::{AgentOrchestratorImpl, ExecutionBackend};
use crate::runtime::subagent::StorageBackedSubagentLookup;
use crate::runtime::trace::ToolTraceRunSink;
use crate::services::adapters::*;
use crate::storage::skill::SkillStorage;
use crate::storage::{
    AgentStorage, BackgroundAgentStorage, ChannelSessionBindingStorage, ChatSessionStorage,
    ConfigStorage, ExecutionTraceStorage, KvStoreStorage, MemoryStorage, SecretStorage,
    TerminalSessionStorage, ToolTraceStorage, TriggerStorage, WorkItemStorage,
};
use restflow_ai::AgentState;
use restflow_ai::agent::{
    StreamEmitter, SubagentConfig, SubagentDefLookup, SubagentDeps, SubagentExecutionBridge,
    SubagentManagerImpl, SubagentTracker, execute_subagent_once,
};
use restflow_ai::llm::{
    CodexClient, DefaultLlmClientFactory, LlmClient, LlmClientFactory, LlmProvider,
    LlmSwitcherImpl, SwappableLlm,
};
use restflow_storage::{AgentDefaults, ApiDefaults, SystemConfig};
use restflow_tools::{
    ListSubagentsTool, ProcessTool, ReplyTool, SpawnSubagentTool, SwitchModelTool,
    ToolRegistryBuilder, WaitSubagentsTool,
};
use restflow_traits::registry::ToolRegistry;
use restflow_traits::security::SecurityGate;
use restflow_traits::store::{ProcessManager, ReplySender};
use restflow_traits::tool::SecretResolver;
use restflow_traits::{ExecutionOutcome, ExecutionPlan};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Debug)]
struct UnavailableReplySender;
const DEFAULT_SECURITY_AGENT_ID: &str = "unknown-agent";
const DEFAULT_SECURITY_TASK_ID: &str = "tool-registry";

impl ReplySender for UnavailableReplySender {
    fn send(&self, _message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async move {
            anyhow::bail!(
                "reply is unavailable in this context. Use an active chat/background session for streamed replies."
            )
        })
    }
}

mod assembly;
mod config;
mod subagent_backend;

use self::config::{
    build_llm_factory, build_switch_model_tool, load_agent_defaults, load_api_defaults,
    load_registry_defaults, load_subagent_config,
};
use self::subagent_backend::create_subagent_manager;

pub use self::assembly::{create_tool_registry, create_tool_registry_with_assessor};

#[cfg(test)]
mod tests;
