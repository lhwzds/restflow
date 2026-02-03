mod core_access;
mod ipc_client;
mod ipc_protocol;
mod ipc_server;
mod launcher;

pub use core_access::CoreAccess;
pub use ipc_client::{IpcClient, is_daemon_available};
pub use ipc_protocol::{IpcRequest, IpcResponse, MAX_MESSAGE_SIZE};
pub use ipc_server::IpcServer;
pub use launcher::{
    DaemonStatus, check_daemon_status, ensure_daemon_running, start_daemon, stop_daemon,
};
