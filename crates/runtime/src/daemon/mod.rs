mod core_access;
mod health;
mod ipc_client;
mod ipc_protocol;
mod ipc_server;
mod launcher;
mod logging;
mod mcp;
mod process;
pub mod recovery;
pub mod request_mapper;
pub(crate) mod session_events;
mod supervisor;
mod task_events;
pub(crate) mod tool_result_mapper;

pub use core_access::CoreAccess;
pub use health::{HealthChecker, HealthStatus, check_health};
pub use ipc_client::{IpcClient, is_daemon_available};
pub use ipc_protocol::{
    IPC_PROTOCOL_VERSION, IpcDaemonStatus, IpcRequest, IpcResponse, IpcStreamEvent,
    MAX_MESSAGE_SIZE, StreamFrame,
};
pub use ipc_server::{
    IpcServer, cancel_foreground_chat_stream, open_foreground_chat_session_stream,
    steer_foreground_chat_stream,
};
pub use launcher::{
    DaemonStatus, check_daemon_status, ensure_daemon_running, ensure_daemon_running_with_config,
    start_daemon, start_daemon_with_config, stop_daemon,
};
pub use logging::{LogPaths, open_daemon_log_append, resolve_log_paths};
pub use mcp::run_mcp_http_server;
pub use process::{DaemonConfig, ProcessManager};
pub use session_events::{ChatSessionEvent, publish_session_event, subscribe_session_events};
pub use supervisor::{Supervisor, SupervisorConfig};
pub use task_events::{publish_task_event, subscribe_task_events};
pub use types::{ToolDefinition, ToolExecutionResult};
