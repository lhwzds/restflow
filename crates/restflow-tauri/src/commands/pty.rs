//! PTY (Pseudo-Terminal) management for interactive shell sessions

use crate::state::AppState;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, State};

/// Maximum size of output history buffer (1MB)
const MAX_HISTORY_SIZE: usize = 1_000_000;

/// Shared state for managing PTY sessions
pub struct PtyState {
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

impl Default for PtyState {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get all running session IDs
    pub fn get_running_session_ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get the output buffer for a session
    pub fn get_output_buffer(&self, session_id: &str) -> Option<String> {
        self.sessions
            .lock()
            .ok()
            .and_then(|s| s.get(session_id).map(|session| session.get_output()))
    }

    /// Remove a session and return its output buffer
    pub fn remove_session(&self, session_id: &str) -> Option<String> {
        self.sessions
            .lock()
            .ok()
            .and_then(|mut s| s.remove(session_id).map(|session| session.get_output()))
    }
}

/// A PTY session with writer handle and output buffer
struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// Accumulated output for history preservation
    output_buffer: Arc<Mutex<String>>,
}

impl PtySession {
    /// Get the current output buffer content
    fn get_output(&self) -> String {
        self.output_buffer
            .lock()
            .map(|b| b.clone())
            .unwrap_or_default()
    }
}

/// Event payload for PTY output
#[derive(Clone, serde::Serialize)]
struct PtyOutputPayload {
    session_id: String,
    data: String,
}

/// Expand tilde (~) in path to actual home directory
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &path[1..]);
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

/// Append data to output buffer with size limit
fn append_to_buffer(buffer: &Arc<Mutex<String>>, data: &str) {
    if let Ok(mut buf) = buffer.lock() {
        buf.push_str(data);
        // Keep last 90% when exceeding max size to avoid frequent truncation
        if buf.len() > MAX_HISTORY_SIZE {
            let keep_from = buf.len() - (MAX_HISTORY_SIZE * 9 / 10);
            *buf = buf[keep_from..].to_string();
        }
    }
}

/// Spawn a new PTY session
#[tauri::command]
pub async fn spawn_pty(
    app: AppHandle,
    state: State<'_, PtyState>,
    app_state: State<'_, crate::AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    // Check if session already exists
    {
        let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        if sessions.contains_key(&session_id) {
            // Already running, just return
            return Ok(());
        }
    }

    // Get session configuration from database
    let terminal_session = app_state
        .core
        .storage
        .terminal_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?;

    let pty_system = native_pty_system();

    // Create PTY with specified size
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {}", e))?;

    // Build shell command based on platform
    let mut cmd = CommandBuilder::new(get_default_shell());

    // Use configured working directory or default to $HOME
    let cwd = terminal_session
        .as_ref()
        .and_then(|s| s.working_directory.clone())
        .map(|p| expand_tilde(&p))
        .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".to_string()));
    cmd.cwd(&cwd);

    // Set TERM environment variable for proper terminal emulation
    cmd.env("TERM", "xterm-256color");

    // Spawn the shell process
    let _child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn shell: {}", e))?;

    // Get writer for sending input to PTY
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {}", e))?;

    // Execute startup command if configured
    if let Some(ref session) = terminal_session
        && let Some(ref startup_cmd) = session.startup_command
        && !startup_cmd.is_empty()
    {
        // Small delay to let shell initialize
        std::thread::sleep(std::time::Duration::from_millis(100));
        let cmd_with_newline = format!("{}\n", startup_cmd);
        let _ = writer.write_all(cmd_with_newline.as_bytes());
        let _ = writer.flush();
    }

    // Clone reader for the output thread
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;

    // Create output buffer
    let output_buffer = Arc::new(Mutex::new(String::new()));
    let buffer_clone = output_buffer.clone();

    // Store session
    let session = PtySession {
        writer,
        master: pair.master,
        output_buffer,
    };

    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(session_id.clone(), session);
    }

    // Spawn thread to read PTY output and emit events
    let session_id_clone = session_id.clone();
    let app_clone = app.clone();

    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    // EOF - PTY closed
                    let _ = app_clone.emit(
                        "pty_closed",
                        PtyOutputPayload {
                            session_id: session_id_clone.clone(),
                            data: String::new(),
                        },
                    );
                    break;
                }
                Ok(n) => {
                    // Convert bytes to string (lossy for non-UTF8)
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();

                    // Emit to frontend
                    let _ = app_clone.emit(
                        "pty_output",
                        PtyOutputPayload {
                            session_id: session_id_clone.clone(),
                            data: data.clone(),
                        },
                    );

                    // Accumulate to history buffer
                    append_to_buffer(&buffer_clone, &data);
                }
                Err(e) => {
                    tracing::error!("PTY read error: {}", e);
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Write data to PTY stdin
#[tauri::command]
pub async fn write_pty(
    state: State<'_, PtyState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;

    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    session
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed to write to PTY: {}", e))?;

    session
        .writer
        .flush()
        .map_err(|e| format!("Failed to flush PTY: {}", e))?;

    Ok(())
}

/// Resize PTY window
#[tauri::command]
pub async fn resize_pty(
    state: State<'_, PtyState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;

    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {}", e))?;

    Ok(())
}

/// Close PTY session
#[tauri::command]
pub async fn close_pty(state: State<'_, PtyState>, session_id: String) -> Result<(), String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;

    if sessions.remove(&session_id).is_some() {
        tracing::info!("Closed PTY session: {}", session_id);
        Ok(())
    } else {
        Err(format!("Session not found: {}", session_id))
    }
}

/// Check if a PTY session is running
#[tauri::command]
pub async fn get_pty_status(
    state: State<'_, PtyState>,
    session_id: String,
) -> Result<bool, String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    Ok(sessions.contains_key(&session_id))
}

/// Get the accumulated output history for a PTY session
///
/// This is used to restore terminal content when reconnecting to a running PTY.
#[tauri::command]
pub async fn get_pty_history(
    state: State<'_, PtyState>,
    session_id: String,
) -> Result<String, String> {
    state
        .get_output_buffer(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

/// Save terminal history for a single session
#[tauri::command]
pub async fn save_terminal_history(
    pty_state: State<'_, PtyState>,
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    // Get current output buffer
    let history = pty_state.get_output_buffer(&session_id);

    if let Some(history) = history {
        // Get and update the terminal session
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
    pty_state: State<'_, PtyState>,
    app_state: State<'_, AppState>,
) -> Result<(), String> {
    let session_ids = pty_state.get_running_session_ids();

    for session_id in session_ids {
        // Get output buffer and remove session
        if let Some(history) = pty_state.remove_session(&session_id) {
            // Update terminal session in storage
            if let Ok(Some(mut session)) = app_state.core.storage.terminal_sessions.get(&session_id)
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

    Ok(())
}

/// Restart a stopped terminal session
#[tauri::command]
pub async fn restart_terminal(
    app_state: State<'_, AppState>,
    session_id: String,
) -> Result<restflow_core::TerminalSession, String> {
    // Get and update the terminal session
    let mut session = app_state
        .core
        .storage
        .terminal_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Terminal session not found: {}", session_id))?;

    // Mark as running (clear history to start fresh)
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
pub fn save_all_terminal_history_sync(pty_state: &PtyState, app_state: &AppState) {
    let session_ids = pty_state.get_running_session_ids();

    for session_id in session_ids {
        if let Some(history) = pty_state.remove_session(&session_id)
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
        // Try to get user's preferred shell from environment
        std::env::var("SHELL")
            .ok()
            .map(|s| {
                // Leak the string to get a static lifetime
                // This is fine since we only call this once per session
                Box::leak(s.into_boxed_str()) as &str
            })
            .unwrap_or("/bin/bash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that PtyState initializes correctly
    #[test]
    fn test_pty_state_new() {
        let state = PtyState::new();
        assert!(state.get_running_session_ids().is_empty());
    }

    /// Test that get_output_buffer returns None for non-existent session
    #[test]
    fn test_get_output_buffer_not_found() {
        let state = PtyState::new();
        assert!(state.get_output_buffer("nonexistent").is_none());
    }

    /// Test that get_running_session_ids returns empty list initially
    #[test]
    fn test_get_running_session_ids_empty() {
        let state = PtyState::new();
        assert!(state.get_running_session_ids().is_empty());
    }

    /// Test that remove_session returns None for non-existent session
    #[test]
    fn test_remove_session_not_found() {
        let state = PtyState::new();
        assert!(state.remove_session("nonexistent").is_none());
    }

    /// Test append_to_buffer basic functionality
    #[test]
    fn test_append_to_buffer_basic() {
        let buffer = Arc::new(Mutex::new(String::new()));

        append_to_buffer(&buffer, "Hello ");
        append_to_buffer(&buffer, "World!");

        let content = buffer.lock().unwrap().clone();
        assert_eq!(content, "Hello World!");
    }

    /// Test append_to_buffer with multiple appends
    #[test]
    fn test_append_to_buffer_multiple() {
        let buffer = Arc::new(Mutex::new(String::new()));

        for i in 0..100 {
            append_to_buffer(&buffer, &format!("line-{}\n", i));
        }

        let content = buffer.lock().unwrap();
        assert!(content.starts_with("line-0\n"));
        assert!(content.ends_with("line-99\n"));
    }

    /// Test buffer size limit enforcement
    #[test]
    fn test_buffer_size_limit() {
        let buffer = Arc::new(Mutex::new(String::new()));

        // Write data larger than MAX_HISTORY_SIZE
        let large_data = "x".repeat(MAX_HISTORY_SIZE + 100);
        append_to_buffer(&buffer, &large_data);

        let content = buffer.lock().unwrap();
        // Should be truncated to approximately 90% of max size
        assert!(content.len() <= MAX_HISTORY_SIZE);
        assert!(content.len() >= MAX_HISTORY_SIZE * 9 / 10);
    }

    /// Test that buffer truncation preserves the end of content
    #[test]
    fn test_buffer_truncation_preserves_end() {
        let buffer = Arc::new(Mutex::new(String::new()));

        // Fill with pattern where we can verify the end is preserved
        let chunk = "ABCDEFGHIJ"; // 10 characters
        let chunks_needed = (MAX_HISTORY_SIZE / 10) + 20; // Exceed limit

        for i in 0..chunks_needed {
            append_to_buffer(&buffer, &format!("{}-{:05}\n", chunk, i));
        }

        let content = buffer.lock().unwrap();
        // The last chunk should be present
        let last_chunk = format!("{}-{:05}\n", chunk, chunks_needed - 1);
        assert!(
            content.ends_with(&last_chunk),
            "Buffer should preserve the most recent content"
        );
    }

    /// Test PtyState default trait
    #[test]
    fn test_pty_state_default() {
        let state = PtyState::default();
        assert!(state.get_running_session_ids().is_empty());
    }

    /// Test expand_tilde with home directory
    #[test]
    fn test_expand_tilde_home() {
        // Save original HOME and set test value
        let original_home = std::env::var("HOME").ok();
        // SAFETY: This is a single-threaded test, so modifying env vars is safe
        unsafe {
            std::env::set_var("HOME", "/Users/test");
        }

        assert_eq!(expand_tilde("~"), "/Users/test");
        assert_eq!(expand_tilde("~/projects"), "/Users/test/projects");
        assert_eq!(expand_tilde("~/a/b/c"), "/Users/test/a/b/c");

        // Restore original HOME
        // SAFETY: This is a single-threaded test, so modifying env vars is safe
        if let Some(home) = original_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        }
    }

    /// Test expand_tilde without tilde
    #[test]
    fn test_expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
        assert_eq!(expand_tilde(""), "");
        assert_eq!(expand_tilde("~suffix"), "~suffix"); // Not ~/
    }
}
