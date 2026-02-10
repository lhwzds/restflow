//! RestFlow Tauri Desktop Application
//!
//! This crate provides the Tauri desktop application wrapper for RestFlow,
//! exposing the workflow engine functionality through Tauri commands.

pub mod agent;
pub mod channel;
pub mod chat;
pub mod commands;
pub mod daemon_manager;
pub mod error;
pub mod executor;
pub mod state;
pub mod subagent;
pub mod webhook;

pub use agent::{
    AgentExecutionEngine, AgentExecutionEngineConfig, BashConfig, BashTool, EmailTool,
    ExecutionResult, FileConfig, FileTool, HttpTool, ListAgentsTool, SpawnAgentTool,
    SpawnTool, SubagentDeps, SubagentSpawner, TelegramTool, Tool, ToolRegistry,
    ToolRegistryBuilder, ToolResult, UseSkillTool, WaitAgentsTool, build_agent_system_prompt,
    default_registry, effective_main_agent_tool_names, main_agent_default_tool_names,
    registry_from_allowlist,
};
pub use channel::{
    BackgroundAgentTrigger, ChatDispatcher, ChatDispatcherConfig, ChatSessionManager,
    MessageDebouncer, MessageHandlerConfig, MessageRouter, RouteDecision, SystemStatus,
    start_message_handler, start_message_handler_with_chat,
};
pub use chat::{
    CHAT_STREAM_EVENT, ChatStreamEvent, ChatStreamKind, ChatStreamState, StepStatus,
    StreamCancelHandle, StreamManager,
};
pub use daemon_manager::DaemonManager;
pub use error::TauriError;
pub use executor::TauriExecutor;
pub use state::{AppBackgroundAgentTrigger, AppState};
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, SpawnHandle, SpawnPriority, SpawnRequest,
    SubagentCompletion, SubagentConfig, SubagentResult, SubagentState, SubagentStatus,
    SubagentTracker, builtin_agents, spawn_subagent,
};
pub use webhook::{
    WebhookServerBuilder, WebhookServerConfig, WebhookServerError, WebhookServerHandle,
    WebhookState,
};
