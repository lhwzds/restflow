//! CLI daemon lifecycle commands for tray dashboard controls.

use crate::daemon_manager::{DaemonLifecycle, DaemonProbeStatus};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CliDaemonLifecycle {
    Running,
    NotRunning,
    Stale,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CliDaemonStatus {
    pub lifecycle: CliDaemonLifecycle,
    pub pid: Option<u32>,
    pub socket_available: bool,
    pub managed_by_tauri: bool,
    pub daemon_status: Option<String>,
    pub daemon_version: Option<String>,
    pub protocol_version: Option<String>,
    pub started_at_ms: Option<i64>,
    pub uptime_secs: Option<u64>,
    pub last_error: Option<String>,
}

impl From<DaemonProbeStatus> for CliDaemonStatus {
    fn from(value: DaemonProbeStatus) -> Self {
        let lifecycle = match value.lifecycle {
            DaemonLifecycle::Running => CliDaemonLifecycle::Running,
            DaemonLifecycle::NotRunning => CliDaemonLifecycle::NotRunning,
            DaemonLifecycle::Stale => CliDaemonLifecycle::Stale,
        };

        Self {
            lifecycle,
            pid: value
                .pid
                .or_else(|| value.ipc_status.as_ref().map(|status| status.pid)),
            socket_available: value.socket_available,
            managed_by_tauri: value.managed_by_tauri,
            daemon_status: value
                .ipc_status
                .as_ref()
                .map(|status| status.status.clone()),
            daemon_version: value
                .ipc_status
                .as_ref()
                .map(|status| status.daemon_version.clone()),
            protocol_version: value
                .ipc_status
                .as_ref()
                .map(|status| status.protocol_version.clone()),
            started_at_ms: value.ipc_status.as_ref().map(|status| status.started_at_ms),
            uptime_secs: value.ipc_status.as_ref().map(|status| status.uptime_secs),
            last_error: value.last_error,
        }
    }
}

#[specta::specta]
#[tauri::command]
pub async fn get_cli_daemon_status(state: State<'_, AppState>) -> Result<CliDaemonStatus, String> {
    let mut daemon = state.daemon.lock().await;
    let status = daemon.probe_status().await.map_err(|e| e.to_string())?;
    Ok(status.into())
}

#[specta::specta]
#[tauri::command]
pub async fn start_cli_daemon(state: State<'_, AppState>) -> Result<CliDaemonStatus, String> {
    let mut daemon = state.daemon.lock().await;
    daemon.start_via_cli().await.map_err(|e| e.to_string())?;
    let status = daemon.probe_status().await.map_err(|e| e.to_string())?;
    Ok(status.into())
}

#[specta::specta]
#[tauri::command]
pub async fn stop_cli_daemon(state: State<'_, AppState>) -> Result<CliDaemonStatus, String> {
    let mut daemon = state.daemon.lock().await;
    daemon.stop_via_cli().await.map_err(|e| e.to_string())?;
    let status = daemon.probe_status().await.map_err(|e| e.to_string())?;
    Ok(status.into())
}

#[specta::specta]
#[tauri::command]
pub async fn restart_cli_daemon(state: State<'_, AppState>) -> Result<CliDaemonStatus, String> {
    let mut daemon = state.daemon.lock().await;
    daemon.restart_via_cli().await.map_err(|e| e.to_string())?;
    let status = daemon.probe_status().await.map_err(|e| e.to_string())?;
    Ok(status.into())
}
