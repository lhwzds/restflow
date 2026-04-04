use super::ipc_protocol::{
    IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent, MAX_MESSAGE_SIZE, StreamFrame,
    ToolDefinition, ToolExecutionResult,
};
use crate::auth::{AuthProfile, AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::daemon::session_events::ChatSessionEvent;
use crate::memory::ExportResult;
use crate::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent,
    BackgroundAgentPatch, BackgroundAgentSpec, ChatMessage, ChatRole, ChatSession,
    ChatSessionSummary, ChatSessionUpdate, ExecutionTraceEvent, ExecutionTraceQuery,
    ExecutionTraceStats, MemoryChunk, MemorySearchResult, MemorySession, MemoryStats,
    RunListQuery, RunSummary, Skill, TerminalSession,
};
use crate::runtime::TaskStreamEvent;
use crate::storage::agent::StoredAgent;
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use std::path::Path;

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::UnixStream;

mod auth;
mod background_agents;
mod memory;
mod sessions;
mod skills;
mod streams;
mod terminal;
mod tools;
mod transport;
#[cfg(not(unix))]
mod unsupported;

#[cfg(unix)]
pub use transport::is_daemon_available;
#[cfg(not(unix))]
pub use unsupported::{IpcClient, is_daemon_available};

#[cfg(unix)]
pub struct IpcClient {
    stream: UnixStream,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use restflow_contracts::ErrorPayload;

    #[test]
    fn format_ipc_error_without_details_uses_simple_message() {
        assert_eq!(
            IpcClient::format_ipc_error(&ErrorPayload::new(500, "boom", None)),
            "IPC error 500: boom"
        );
    }

    #[test]
    fn format_ipc_error_with_details_serializes_json() {
        let formatted = IpcClient::format_ipc_error(&ErrorPayload::new(
            400,
            "bad request",
            Some(serde_json::json!({ "field": "agent_id" })),
        ));

        assert!(formatted.contains('"'.to_string().as_str()));
        assert!(formatted.contains("bad request"));
        assert!(formatted.contains("agent_id"));
    }
}
