//! RestFlow Tauri Desktop Application
//!
//! This crate provides the Tauri desktop application wrapper for RestFlow,
//! exposing the workflow engine functionality through Tauri commands.

pub mod agent;
pub mod agent_task;
pub mod channel;
pub mod chat;
pub mod commands;
pub mod daemon_manager;
pub mod error;
pub mod executor;
pub mod mcp;
pub mod state;
pub mod subagent;
pub mod webhook;

pub use agent::{
    BashConfig, BashTool, EmailTool, ExecutionResult, FileConfig, FileTool, HttpTool,
    ListAgentsTool, PythonTool, SpawnAgentTool, SpawnTool, SubagentDeps, SubagentSpawner,
    TelegramTool, Tool, ToolRegistry, ToolRegistryBuilder, ToolResult, UnifiedAgent,
    UnifiedAgentConfig, UseSkillTool, WaitAgentsTool, build_agent_system_prompt, default_registry,
    effective_main_agent_tool_names, main_agent_default_tool_names, registry_from_allowlist,
};
pub use agent_task::{
    AgentExecutor, AgentTaskRunner, HEARTBEAT_EVENT, HeartbeatEmitter, HeartbeatEvent,
    HeartbeatPulse, NoopHeartbeatEmitter, RealAgentExecutor, RunnerConfig, RunnerHandle,
    RunnerStatus, TauriHeartbeatEmitter, TelegramNotifier,
};
pub use channel::{
    ChatDispatcher, ChatDispatcherConfig, ChatSessionManager, MessageDebouncer,
    MessageHandlerConfig, MessageRouter, RouteDecision, SystemStatus, TaskTrigger,
    start_message_handler, start_message_handler_with_chat,
};
pub use chat::{
    CHAT_STREAM_EVENT, ChatStreamEvent, ChatStreamKind, ChatStreamState, StepStatus,
    StreamCancelHandle, StreamManager,
};
pub use daemon_manager::DaemonManager;
pub use error::TauriError;
pub use executor::TauriExecutor;
pub use mcp::RestFlowMcpServer;
pub use state::{AppState, AppTaskTrigger};
pub use subagent::{
    AgentDefinition, AgentDefinitionRegistry, SpawnHandle, SpawnPriority, SpawnRequest,
    SubagentCompletion, SubagentConfig, SubagentResult, SubagentState, SubagentStatus,
    SubagentTracker, builtin_agents, spawn_subagent,
};
pub use webhook::{
    WebhookServerBuilder, WebhookServerConfig, WebhookServerError, WebhookServerHandle,
    WebhookState,
};
