//! Agent Task Tauri commands
//!
//! Provides IPC commands for managing scheduled agent tasks from the frontend.

use crate::state::AppState;
use restflow_core::models::{AgentTask, AgentTaskStatus, NotificationConfig, TaskEvent, TaskSchedule};
use serde::Deserialize;
use tauri::State;

/// Request to create a new agent task
#[derive(Debug, Deserialize)]
pub struct CreateAgentTaskRequest {
    /// Display name of the task
    pub name: String,
    /// ID of the agent to execute
    pub agent_id: String,
    /// Schedule configuration
    pub schedule: TaskSchedule,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Optional input/prompt to send to the agent
    #[serde(default)]
    pub input: Option<String>,
    /// Optional notification configuration
    #[serde(default)]
    pub notification: Option<NotificationConfig>,
}

/// Request to update an existing agent task
#[derive(Debug, Deserialize)]
pub struct UpdateAgentTaskRequest {
    /// New display name (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// New description (optional)
    #[serde(default)]
    pub description: Option<String>,
    /// New agent ID (optional)
    #[serde(default)]
    pub agent_id: Option<String>,
    /// New input/prompt (optional)
    #[serde(default)]
    pub input: Option<String>,
    /// New schedule (optional)
    #[serde(default)]
    pub schedule: Option<TaskSchedule>,
    /// New notification config (optional)
    #[serde(default)]
    pub notification: Option<NotificationConfig>,
}

/// List all agent tasks
#[tauri::command]
pub async fn list_agent_tasks(state: State<'_, AppState>) -> Result<Vec<AgentTask>, String> {
    state
        .core
        .storage
        .agent_tasks
        .list_tasks()
        .map_err(|e| e.to_string())
}

/// List agent tasks filtered by status
#[tauri::command]
pub async fn list_agent_tasks_by_status(
    state: State<'_, AppState>,
    status: AgentTaskStatus,
) -> Result<Vec<AgentTask>, String> {
    state
        .core
        .storage
        .agent_tasks
        .list_tasks_by_status(status)
        .map_err(|e| e.to_string())
}

/// Get an agent task by ID
#[tauri::command]
pub async fn get_agent_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<AgentTask, String> {
    state
        .core
        .storage
        .agent_tasks
        .get_task(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent task '{}' not found", id))
}

/// Create a new agent task
#[tauri::command]
pub async fn create_agent_task(
    state: State<'_, AppState>,
    request: CreateAgentTaskRequest,
) -> Result<AgentTask, String> {
    // Create the task with basic info
    let mut task = state
        .core
        .storage
        .agent_tasks
        .create_task(request.name, request.agent_id, request.schedule)
        .map_err(|e| e.to_string())?;

    // Apply optional fields if provided
    let mut needs_update = false;

    if let Some(description) = request.description {
        task.description = Some(description);
        needs_update = true;
    }

    if let Some(input) = request.input {
        task.input = Some(input);
        needs_update = true;
    }

    if let Some(notification) = request.notification {
        task.notification = notification;
        needs_update = true;
    }

    // Update if we modified any optional fields
    if needs_update {
        state
            .core
            .storage
            .agent_tasks
            .update_task(&task)
            .map_err(|e| e.to_string())?;
    }

    Ok(task)
}

/// Update an existing agent task
#[tauri::command]
pub async fn update_agent_task(
    state: State<'_, AppState>,
    id: String,
    request: UpdateAgentTaskRequest,
) -> Result<AgentTask, String> {
    // Get existing task
    let mut task = state
        .core
        .storage
        .agent_tasks
        .get_task(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent task '{}' not found", id))?;

    // Apply updates
    if let Some(name) = request.name {
        task.name = name;
    }

    if let Some(description) = request.description {
        task.description = Some(description);
    }

    if let Some(agent_id) = request.agent_id {
        task.agent_id = agent_id;
    }

    if let Some(input) = request.input {
        task.input = Some(input);
    }

    if let Some(schedule) = request.schedule {
        task.schedule = schedule;
        // Recalculate next run time when schedule changes
        task.update_next_run();
    }

    if let Some(notification) = request.notification {
        task.notification = notification;
    }

    // Update timestamp
    task.updated_at = chrono::Utc::now().timestamp_millis();

    // Save changes
    state
        .core
        .storage
        .agent_tasks
        .update_task(&task)
        .map_err(|e| e.to_string())?;

    Ok(task)
}

/// Delete an agent task
#[tauri::command]
pub async fn delete_agent_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    state
        .core
        .storage
        .agent_tasks
        .delete_task(&id)
        .map_err(|e| e.to_string())
}

/// Pause an agent task
#[tauri::command]
pub async fn pause_agent_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<AgentTask, String> {
    state
        .core
        .storage
        .agent_tasks
        .pause_task(&id)
        .map_err(|e| e.to_string())
}

/// Resume a paused agent task
#[tauri::command]
pub async fn resume_agent_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<AgentTask, String> {
    state
        .core
        .storage
        .agent_tasks
        .resume_task(&id)
        .map_err(|e| e.to_string())
}

/// Get events for a specific task
#[tauri::command]
pub async fn get_agent_task_events(
    state: State<'_, AppState>,
    task_id: String,
    limit: Option<usize>,
) -> Result<Vec<TaskEvent>, String> {
    let events = if let Some(limit) = limit {
        state
            .core
            .storage
            .agent_tasks
            .list_recent_events_for_task(&task_id, limit)
    } else {
        state
            .core
            .storage
            .agent_tasks
            .list_events_for_task(&task_id)
    };

    events.map_err(|e| e.to_string())
}

/// Get runnable tasks (tasks that should run now based on schedule)
#[tauri::command]
pub async fn get_runnable_agent_tasks(
    state: State<'_, AppState>,
) -> Result<Vec<AgentTask>, String> {
    let current_time = chrono::Utc::now().timestamp_millis();
    state
        .core
        .storage
        .agent_tasks
        .list_runnable_tasks(current_time)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[allow(dead_code)]
    async fn create_test_state() -> AppState {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();
        AppState::new(db_path_str).await.unwrap()
    }

    #[tokio::test]
    async fn test_create_agent_task_request_deserialize() {
        let json = r#"{
            "name": "Test Task",
            "agent_id": "agent-001",
            "schedule": { "type": "interval", "interval_ms": 3600000 }
        }"#;

        let request: CreateAgentTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, "Test Task");
        assert_eq!(request.agent_id, "agent-001");
        assert!(request.description.is_none());
        assert!(request.input.is_none());
        assert!(request.notification.is_none());
    }

    #[tokio::test]
    async fn test_create_agent_task_request_with_all_fields() {
        let json = r#"{
            "name": "Full Task",
            "agent_id": "agent-002",
            "schedule": { "type": "once", "run_at": 1704067200000 },
            "description": "A complete task",
            "input": "Hello agent",
            "notification": {
                "telegram_enabled": true,
                "telegram_chat_id": "123456"
            }
        }"#;

        let request: CreateAgentTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, "Full Task");
        assert_eq!(request.agent_id, "agent-002");
        assert_eq!(request.description, Some("A complete task".to_string()));
        assert_eq!(request.input, Some("Hello agent".to_string()));
        assert!(request.notification.is_some());
        let notif = request.notification.unwrap();
        assert!(notif.telegram_enabled);
        assert_eq!(notif.telegram_chat_id, Some("123456".to_string()));
    }

    #[tokio::test]
    async fn test_update_agent_task_request_partial() {
        let json = r#"{
            "name": "New Name"
        }"#;

        let request: UpdateAgentTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, Some("New Name".to_string()));
        assert!(request.description.is_none());
        assert!(request.agent_id.is_none());
        assert!(request.input.is_none());
        assert!(request.schedule.is_none());
        assert!(request.notification.is_none());
    }

    #[tokio::test]
    async fn test_cron_schedule_deserialize() {
        let json = r#"{
            "name": "Cron Task",
            "agent_id": "agent-003",
            "schedule": {
                "type": "cron",
                "expression": "0 9 * * *",
                "timezone": "America/Los_Angeles"
            }
        }"#;

        let request: CreateAgentTaskRequest = serde_json::from_str(json).unwrap();
        match request.schedule {
            TaskSchedule::Cron { expression, timezone } => {
                assert_eq!(expression, "0 9 * * *");
                assert_eq!(timezone, Some("America/Los_Angeles".to_string()));
            }
            _ => panic!("Expected Cron schedule"),
        }
    }
}
