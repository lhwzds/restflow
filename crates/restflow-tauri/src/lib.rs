//! RestFlow Tauri Desktop Application
//!
//! This crate provides the Tauri desktop application wrapper for RestFlow,
//! exposing the workflow engine functionality through Tauri commands.

pub mod agent_task;
pub mod commands;
pub mod error;
pub mod mcp;
pub mod state;

pub use agent_task::{
    AgentTaskRunner, HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse, NoopHeartbeatEmitter,
    RealAgentExecutor, RunnerConfig, RunnerHandle, RunnerStatus, TauriHeartbeatEmitter,
    TelegramNotifier, HEARTBEAT_EVENT,
};
pub use error::TauriError;
pub use mcp::RestFlowMcpServer;
pub use state::AppState;
