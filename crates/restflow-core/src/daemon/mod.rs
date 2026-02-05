mod core_access;
mod health;
mod http;
mod ipc_client;
mod ipc_protocol;
mod ipc_server;
mod launcher;
mod logging;
mod mcp;
mod process;
mod supervisor;

pub use core_access::CoreAccess;
pub use health::{HealthChecker, HealthStatus, check_health};
pub use http::{HttpConfig, HttpServer};
pub use ipc_client::{IpcClient, is_daemon_available};
pub use ipc_protocol::{IpcRequest, IpcResponse, StreamFrame, MAX_MESSAGE_SIZE};
pub use ipc_server::IpcServer;
pub use launcher::{
    DaemonStatus, check_daemon_status, ensure_daemon_running, ensure_daemon_running_with_config,
    start_daemon, start_daemon_with_config, stop_daemon,
};
pub use logging::{LogPaths, open_daemon_log_append, resolve_log_paths};
pub use mcp::run_mcp_http_server;
pub use process::{DaemonConfig, ProcessManager};
pub use supervisor::{Supervisor, SupervisorConfig};
