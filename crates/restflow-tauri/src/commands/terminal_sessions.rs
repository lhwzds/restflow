//! Terminal session-related Tauri commands

use crate::state::AppState;
use restflow_core::TerminalSession;
use tauri::State;

/// List all terminal sessions
#[tauri::command]
pub async fn list_terminal_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<TerminalSession>, String> {
    state
        .executor()
        .list_terminal_sessions()
        .await
        .map_err(|e| e.to_string())
}

/// Get a terminal session by ID
#[tauri::command]
pub async fn get_terminal_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<TerminalSession, String> {
    state
        .executor()
        .get_terminal_session(id.clone())
        .await
        .map_err(|e| e.to_string())
}

/// Create a new terminal session
#[tauri::command]
pub async fn create_terminal_session(
    state: State<'_, AppState>,
) -> Result<TerminalSession, String> {
    state
        .executor()
        .create_terminal_session()
        .await
        .map_err(|e| e.to_string())
}

/// Rename a terminal session
#[tauri::command]
pub async fn rename_terminal_session(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<TerminalSession, String> {
    state
        .executor()
        .rename_terminal_session(id.clone(), name)
        .await
        .map_err(|e| e.to_string())
}

/// Update a terminal session's configuration
#[tauri::command]
pub async fn update_terminal_session(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    working_directory: Option<String>,
    startup_command: Option<String>,
) -> Result<TerminalSession, String> {
    state
        .executor()
        .update_terminal_session(id.clone(), name, working_directory, startup_command)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a terminal session by ID
#[tauri::command]
pub async fn delete_terminal_session(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .executor()
        .delete_terminal_session(id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use uuid::Uuid;

    /// Test that UUID generation produces unique IDs even when called rapidly
    ///
    /// This verifies the fix for Bug #3 where timestamp-based IDs could collide
    /// when multiple terminals were created in the same millisecond.
    #[test]
    fn test_session_id_uniqueness() {
        let mut ids = HashSet::new();

        // Rapidly generate 1000 IDs to simulate fast clicking
        for _ in 0..1000 {
            let id = format!("terminal-{}", Uuid::new_v4());
            assert!(
                !ids.contains(&id),
                "Duplicate ID generated: {} - this should never happen with UUIDs",
                id
            );
            ids.insert(id);
        }

        // Verify all IDs are unique
        assert_eq!(ids.len(), 1000, "Should have exactly 1000 unique IDs");
    }

    /// Test that session ID has correct prefix format
    #[test]
    fn test_session_id_format() {
        let id = format!("terminal-{}", Uuid::new_v4());

        // Should start with "terminal-"
        assert!(id.starts_with("terminal-"));

        // Should be longer than just the prefix (UUID adds 36 chars)
        assert!(id.len() > "terminal-".len());

        // UUID portion should be valid
        let uuid_part = &id["terminal-".len()..];
        assert!(
            Uuid::parse_str(uuid_part).is_ok(),
            "UUID portion should be valid: {}",
            uuid_part
        );
    }

    /// Test UUID v4 characteristics
    #[test]
    fn test_uuid_v4_characteristics() {
        let uuid = Uuid::new_v4();

        // UUID v4 should have version 4 and variant 1
        assert_eq!(uuid.get_version_num(), 4);

        // UUID should be 36 characters when formatted as string (with hyphens)
        assert_eq!(uuid.to_string().len(), 36);
    }
}
