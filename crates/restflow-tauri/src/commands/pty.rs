//! PTY (Pseudo-Terminal) management for interactive shell sessions

use crate::state::AppState;
use portable_pty::PtySize;
use restflow_core::process::{
    ProcessOutputListener, ProcessSessionSource, ProcessShellOptions, ProcessSpawnOptions,
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Event payload for PTY output
#[derive(Clone, serde::Serialize)]
struct PtyOutputPayload {
    session_id: String,
    data: String,
}

struct TauriPtyOutputListener {
    app: AppHandle,
}

impl TauriPtyOutputListener {
    fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ProcessOutputListener for TauriPtyOutputListener {
    fn on_output(&self, session_id: &str, data: &str) {
        let _ = self.app.emit(
            "pty_output",
            PtyOutputPayload {
                session_id: session_id.to_string(),
                data: data.to_string(),
            },
        );
    }

    fn on_closed(&self, session_id: &str) {
        let _ = self.app.emit(
            "pty_closed",
            PtyOutputPayload {
                session_id: session_id.to_string(),
                data: String::new(),
            },
        );
    }
}

/// Expand tilde (~) in path to actual home directory
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{}{}", home, &path[1..]);
    } else if path == "~"
        && let Ok(home) = std::env::var("HOME")
    {
        return home;
    }
    path.to_string()
}

/// Spawn a new PTY session
#[tauri::command]
pub async fn spawn_pty(
    app: AppHandle,
    app_state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if app_state.process_registry.has_session(&session_id) {
        return Ok(());
    }

    let terminal_session = app_state
        .core
        .storage
        .terminal_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?;

    let cwd = terminal_session
        .as_ref()
        .and_then(|s| s.working_directory.clone())
        .map(|p| expand_tilde(&p))
        .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".to_string()));

    let spawn = ProcessSpawnOptions {
        session_id: Some(session_id.clone()),
        cwd: Some(cwd),
        source: ProcessSessionSource::User,
        pty_size: PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        output_listener: Some(Arc::new(TauriPtyOutputListener::new(app.clone()))),
        ..Default::default()
    };

    let options = ProcessShellOptions {
        spawn,
        startup_command: terminal_session
            .as_ref()
            .and_then(|session| session.startup_command.clone()),
    };

    app_state
        .process_registry
        .spawn_shell(get_default_shell(), options)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Write data to PTY stdin
#[tauri::command]
pub async fn write_pty(
    app_state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    app_state
        .process_registry
        .write(&session_id, &data)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Resize PTY window
#[tauri::command]
pub async fn resize_pty(
    app_state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    app_state
        .process_registry
        .resize(
            &session_id,
            PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Close PTY session and update database status
#[tauri::command]
pub async fn close_pty(
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let history = app_state.process_registry.remove_session(&session_id);

    if history.is_some() {
        tracing::info!("Closed PTY session: {}", session_id);

        if let Ok(Some(mut session)) = app_state.core.storage.terminal_sessions.get(&session_id) {
            session.set_stopped(history);
            if let Err(e) = app_state
                .core
                .storage
                .terminal_sessions
                .update(&session_id, &session)
            {
                tracing::warn!("Failed to update terminal session status: {}", e);
            }
        }

        Ok(())
    } else {
        Err(format!("PTY session not found: {}", session_id))
    }
}

/// Check if a PTY session is running
#[tauri::command]
pub async fn get_pty_status(
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    Ok(app_state.process_registry.has_session(&session_id))
}

/// Get the accumulated output history for a PTY session
#[tauri::command]
pub async fn get_pty_history(
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    app_state
        .process_registry
        .get_output_buffer(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

/// Save terminal history for a single session
#[tauri::command]
pub async fn save_terminal_history(
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let history = app_state.process_registry.get_output_buffer(&session_id);

    if let Some(history) = history {
        let mut session = app_state
            .core
            .storage
            .terminal_sessions
            .get(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Terminal session not found: {}", session_id))?;

        session.update_history(history);

        app_state
            .core
            .storage
            .terminal_sessions
            .update(&session_id, &session)
            .map_err(|e| e.to_string())?;

        tracing::debug!("Saved history for terminal: {}", session_id);
    }

    Ok(())
}

/// Save history for all running terminals (called on app close)
#[tauri::command]
pub async fn save_all_terminal_history(
    app_state: State<'_, AppState>,
) -> Result<(), String> {
    let session_ids = app_state
        .process_registry
        .list_session_ids_by_source(ProcessSessionSource::User);

    for session_id in session_ids {
        if let Some(history) = app_state.process_registry.remove_session(&session_id)
            && let Ok(Some(mut session)) =
                app_state.core.storage.terminal_sessions.get(&session_id)
        {
            session.set_stopped(Some(history));

            if let Err(e) = app_state
                .core
                .storage
                .terminal_sessions
                .update(&session_id, &session)
            {
                tracing::error!("Failed to save terminal {}: {}", session_id, e);
            } else {
                tracing::info!("Saved and stopped terminal: {}", session_id);
            }
        }
    }

    Ok(())
}

/// Restart a stopped terminal session
#[tauri::command]
pub async fn restart_terminal(
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<restflow_core::TerminalSession, String> {
    let mut session = app_state
        .core
        .storage
        .terminal_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Terminal session not found: {}", session_id))?;

    session.set_running();
    session.history = None;

    app_state
        .core
        .storage
        .terminal_sessions
        .update(&session_id, &session)
        .map_err(|e| e.to_string())?;

    tracing::info!("Restarted terminal: {}", session_id);
    Ok(session)
}

/// Synchronous version of save_all_terminal_history for use in window close handler
pub fn save_all_terminal_history_sync(app_state: &AppState) {
    let session_ids = app_state
        .process_registry
        .list_session_ids_by_source(ProcessSessionSource::User);

    for session_id in session_ids {
        if let Some(history) = app_state.process_registry.remove_session(&session_id)
            && let Ok(Some(mut session)) =
                app_state.core.storage.terminal_sessions.get(&session_id)
        {
            session.set_stopped(Some(history));

            if let Err(e) = app_state
                .core
                .storage
                .terminal_sessions
                .update(&session_id, &session)
            {
                tracing::error!("Failed to save terminal {}: {}", session_id, e);
            } else {
                tracing::info!("Saved and stopped terminal: {}", session_id);
            }
        }
    }
}

/// Get the default shell for the current platform
fn get_default_shell() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "powershell.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL")
            .ok()
            .map(|s| Box::leak(s.into_boxed_str()) as &str)
            .unwrap_or("/bin/bash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde_home() {
        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", "/Users/test");
        }

        assert_eq!(expand_tilde("~"), "/Users/test");
        assert_eq!(expand_tilde("~/projects"), "/Users/test/projects");
        assert_eq!(expand_tilde("~/a/b/c"), "/Users/test/a/b/c");

        if let Some(home) = original_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        }
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
        assert_eq!(expand_tilde(""), "");
        assert_eq!(expand_tilde("~suffix"), "~suffix");
    }
}
