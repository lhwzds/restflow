mod core_access;
mod health;
mod ipc_client;
mod ipc_protocol;
mod ipc_server;
mod launcher;
mod logging;
mod process;
mod supervisor;

pub use core_access::CoreAccess;
pub use health::{HealthChecker, HealthStatus, check_health};
pub use ipc_client::{IpcClient, is_daemon_available};
pub use ipc_protocol::{IpcRequest, IpcResponse, MAX_MESSAGE_SIZE};
pub use ipc_server::IpcServer;
pub use launcher::{
    DaemonStatus, check_daemon_status, ensure_daemon_running, ensure_daemon_running_with_config,
    start_daemon, start_daemon_with_config, stop_daemon,
};
pub use logging::{LogPaths, open_daemon_log_append, resolve_log_paths};
pub use process::{DaemonConfig, ProcessManager};
pub use supervisor::{Supervisor, SupervisorConfig};
