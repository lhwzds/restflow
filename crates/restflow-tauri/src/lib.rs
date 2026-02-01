//! RestFlow Tauri Desktop Application
//!
//! This crate provides the Tauri desktop application wrapper for RestFlow,
//! exposing the workflow engine functionality through Tauri commands.

pub mod agent_task;
pub mod channel;
pub mod chat;
pub mod commands;
pub mod error;
pub mod main_agent;
pub mod mcp;
pub mod state;
pub mod webhook;

pub use agent_task::{
    AgentExecutor, AgentTaskRunner, HEARTBEAT_EVENT, HeartbeatEmitter, HeartbeatEvent,
    HeartbeatPulse, NoopHeartbeatEmitter, RealAgentExecutor, RunnerConfig, RunnerHandle,
    RunnerStatus, TauriHeartbeatEmitter, TelegramNotifier,
};
pub use channel::{start_message_handler, MessageHandlerConfig, SystemStatus, TaskTrigger};
pub use chat::{
    ChatStreamEvent, ChatStreamKind, ChatStreamState, StepStatus, StreamCancelHandle,
    StreamManager, CHAT_STREAM_EVENT,
};
pub use error::TauriError;
pub use main_agent::{
    AgentDefinition, AgentDefinitionRegistry, AgentSession, MAIN_AGENT_EVENT, MainAgent,
    MainAgentConfig, MainAgentEvent, MainAgentEventEmitter, MainAgentEventKind,
    NoopMainAgentEmitter, SessionMessage, SpawnHandle, SpawnRequest, SubagentResult, SubagentState,
    SubagentStatus, SubagentTracker, TauriMainAgentEmitter,
};
pub use mcp::RestFlowMcpServer;
pub use state::{AppState, AppTaskTrigger};
pub use webhook::{
    WebhookServerBuilder, WebhookServerConfig, WebhookServerError, WebhookServerHandle,
    WebhookState,
};
