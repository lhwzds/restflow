//! Terminal Session model for persistent terminal sessions.
//! Terminal sessions are stored like files, allowing them to be reopened after closing.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Terminal session status
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TerminalStatus {
    /// Terminal is running (PTY process active)
    Running,
    /// Terminal is stopped (PTY process terminated, history preserved)
    #[default]
    Stopped,
}

/// A terminal session represents a persistent terminal instance
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TerminalSession {
    /// Unique identifier for the session
    pub id: String,
    /// Display name of the session (e.g., "Terminal 1")
    pub name: String,
    /// Timestamp when the session was created (milliseconds since epoch)
    #[ts(type = "number")]
    pub created_at: i64,
    /// Current status of the terminal
    #[serde(default)]
    pub status: TerminalStatus,
    /// Terminal output history (populated when stopped or during auto-save)
    #[serde(default)]
    pub history: Option<String>,
    /// Timestamp when the session was last stopped/saved (milliseconds since epoch)
    #[serde(default)]
    #[ts(type = "number | null")]
    pub stopped_at: Option<i64>,
    /// Working directory for the terminal (default: $HOME)
    #[serde(default)]
    pub working_directory: Option<String>,
    /// Command to execute after terminal starts
    #[serde(default)]
    pub startup_command: Option<String>,
}

impl TerminalSession {
    /// Create a new terminal session with the given parameters
    pub fn new(id: String, name: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            name,
            created_at: now,
            status: TerminalStatus::Running,
            history: None,
            stopped_at: None,
            working_directory: None,
            startup_command: None,
        }
    }

    /// Update the session's startup configuration
    pub fn set_config(
        &mut self,
        working_directory: Option<String>,
        startup_command: Option<String>,
    ) {
        self.working_directory = working_directory;
        self.startup_command = startup_command;
    }

    /// Update the session's name
    pub fn rename(&mut self, name: String) {
        self.name = name;
    }

    /// Mark the session as running
    pub fn set_running(&mut self) {
        self.status = TerminalStatus::Running;
    }

    /// Mark the session as stopped and save history
    pub fn set_stopped(&mut self, history: Option<String>) {
        self.status = TerminalStatus::Stopped;
        self.history = history;
        self.stopped_at = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Update the history (for periodic auto-save)
    pub fn update_history(&mut self, history: String) {
        self.history = Some(history);
        self.stopped_at = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Check if the session is running
    pub fn is_running(&self) -> bool {
        self.status == TerminalStatus::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_session_new() {
        let session = TerminalSession::new("terminal-123".to_string(), "Terminal 1".to_string());

        assert_eq!(session.id, "terminal-123");
        assert_eq!(session.name, "Terminal 1");
        assert!(session.created_at > 0);
        assert_eq!(session.status, TerminalStatus::Running);
        assert!(session.history.is_none());
        assert!(session.stopped_at.is_none());
    }

    #[test]
    fn test_terminal_session_rename() {
        let mut session =
            TerminalSession::new("terminal-123".to_string(), "Terminal 1".to_string());

        session.rename("My Custom Terminal".to_string());
        assert_eq!(session.name, "My Custom Terminal");
    }

    #[test]
    fn test_terminal_session_status() {
        let mut session =
            TerminalSession::new("terminal-123".to_string(), "Terminal 1".to_string());

        assert!(session.is_running());

        session.set_stopped(Some("test history".to_string()));
        assert!(!session.is_running());
        assert_eq!(session.status, TerminalStatus::Stopped);
        assert_eq!(session.history, Some("test history".to_string()));
        assert!(session.stopped_at.is_some());

        session.set_running();
        assert!(session.is_running());
    }

    #[test]
    fn test_terminal_session_update_history() {
        let mut session =
            TerminalSession::new("terminal-123".to_string(), "Terminal 1".to_string());

        session.update_history("new history".to_string());
        assert_eq!(session.history, Some("new history".to_string()));
        assert!(session.stopped_at.is_some());
    }

    #[test]
    fn test_terminal_status_default() {
        // Test serde default for old data migration
        let status: TerminalStatus = Default::default();
        assert_eq!(status, TerminalStatus::Stopped);
    }

    #[test]
    fn test_terminal_session_config() {
        let mut session =
            TerminalSession::new("terminal-123".to_string(), "Terminal 1".to_string());

        // Initially no config
        assert!(session.working_directory.is_none());
        assert!(session.startup_command.is_none());

        // Set config
        session.set_config(
            Some("/home/user/projects".to_string()),
            Some("ls -la".to_string()),
        );

        assert_eq!(
            session.working_directory,
            Some("/home/user/projects".to_string())
        );
        assert_eq!(session.startup_command, Some("ls -la".to_string()));

        // Clear config
        session.set_config(None, None);
        assert!(session.working_directory.is_none());
        assert!(session.startup_command.is_none());
    }
}
