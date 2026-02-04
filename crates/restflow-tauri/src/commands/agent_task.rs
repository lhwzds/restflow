//! Agent Task Tauri commands
//!
//! Provides IPC commands for managing scheduled agent tasks from the frontend.
//!
//! # Streaming Events
//!
//! The `run_agent_task_streaming` command executes a task immediately and streams
//! real-time events to the frontend via Tauri's event system. Frontend should
//! listen to the `agent-task:stream` event to receive `TaskStreamEvent` updates.
//!
//! ```typescript
//! import { listen } from '@tauri-apps/api/event';
//! import type { TaskStreamEvent } from './types/generated';
//!
//! const unlisten = await listen<TaskStreamEvent>('agent-task:stream', (event) => {
//!   console.log('Task event:', event.payload);
//! });
//! ```

use crate::agent_task::{TASK_STREAM_EVENT, TaskStreamEvent, TauriEventEmitter};
use crate::state::AppState;
use restflow_core::models::{
    AgentTask, AgentTaskStatus, ExecutionMode, NotificationConfig, TaskEvent, TaskSchedule,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

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
    /// Optional execution mode (API or CLI)
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
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
        .executor()
        .list_tasks()
        .await
        .map_err(|e| e.to_string())
}

/// List agent tasks filtered by status
#[tauri::command]
pub async fn list_agent_tasks_by_status(
    state: State<'_, AppState>,
    status: AgentTaskStatus,
) -> Result<Vec<AgentTask>, String> {
    let status_str = match status {
        AgentTaskStatus::Active => "active",
        AgentTaskStatus::Paused => "paused",
        AgentTaskStatus::Running => "running",
        AgentTaskStatus::Completed => "completed",
        AgentTaskStatus::Failed => "failed",
    };
    state
        .executor()
        .list_tasks_by_status(status_str.to_string())
        .await
        .map_err(|e| e.to_string())
}

/// Get an agent task by ID
#[tauri::command]
pub async fn get_agent_task(state: State<'_, AppState>, id: String) -> Result<AgentTask, String> {
    state
        .executor()
        .get_task(id.clone())
        .await
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
        .executor()
        .create_task(request.name, request.agent_id, request.schedule)
        .await
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

    if let Some(execution_mode) = request.execution_mode {
        task.execution_mode = execution_mode;
        needs_update = true;
    }

    // Update if we modified any optional fields
    if needs_update {
        task = state
            .executor()
            .update_task(task)
            .await
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
        .executor()
        .get_task(id.clone())
        .await
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

    // Save changes via executor
    let task = state
        .executor()
        .update_task(task)
        .await
        .map_err(|e| e.to_string())?;

    Ok(task)
}

/// Delete an agent task
#[tauri::command]
pub async fn delete_agent_task(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state
        .executor()
        .delete_task(id)
        .await
        .map_err(|e| e.to_string())
}

/// Pause an agent task
#[tauri::command]
pub async fn pause_agent_task(state: State<'_, AppState>, id: String) -> Result<AgentTask, String> {
    state
        .executor()
        .pause_task(id)
        .await
        .map_err(|e| e.to_string())
}

/// Resume a paused agent task
#[tauri::command]
pub async fn resume_agent_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<AgentTask, String> {
    state
        .executor()
        .resume_task(id)
        .await
        .map_err(|e| e.to_string())
}

/// Cancel a running agent task
#[tauri::command]
pub async fn cancel_agent_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<bool, String> {
    state
        .cancel_task(task_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(true)
}

/// Get events for a specific task
#[tauri::command]
pub async fn get_agent_task_events(
    state: State<'_, AppState>,
    task_id: String,
    limit: Option<usize>,
) -> Result<Vec<TaskEvent>, String> {
    let mut events = state
        .executor()
        .get_task_history(task_id)
        .await
        .map_err(|e| e.to_string())?;

    // Apply limit if specified (IPC doesn't support limit natively)
    if let Some(limit) = limit {
        events.truncate(limit);
    }

    Ok(events)
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

// ============================================================================
// Streaming Event Commands
// ============================================================================

/// Response for streaming task execution
#[derive(Debug, Clone, Serialize)]
pub struct StreamingTaskResponse {
    /// Task ID that was started
    pub task_id: String,
    /// Event channel name to listen on
    pub event_channel: String,
    /// Whether the task is already running (queued)
    pub already_running: bool,
}

/// Run an agent task immediately and stream events to the frontend.
///
/// This command triggers immediate execution of a task and emits real-time
/// events via Tauri's event system. The frontend should listen to the
/// `agent-task:stream` event to receive `TaskStreamEvent` updates.
///
/// # Arguments
///
/// * `id` - The ID of the agent task to run
///
/// # Returns
///
/// Returns a `StreamingTaskResponse` with the task ID and event channel name.
///
/// # Events
///
/// The following events are emitted on the `agent-task:stream` channel:
/// - `started` - Task execution has begun
/// - `output` - Output from the task (stdout/stderr)
/// - `progress` - Progress updates for long-running tasks
/// - `completed` - Task finished successfully
/// - `failed` - Task failed with an error
/// - `cancelled` - Task was cancelled (timeout or user request)
/// - `heartbeat` - Periodic heartbeat while task is running
///
/// # Example (Frontend)
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
/// import { listen } from '@tauri-apps/api/event';
///
/// // Start listening before invoking
/// const unlisten = await listen<TaskStreamEvent>('agent-task:stream', (event) => {
///   if (event.payload.task_id === taskId) {
///     switch (event.payload.kind.type) {
///       case 'started':
///         console.log('Task started:', event.payload.kind.task_name);
///         break;
///       case 'output':
///         console.log('Output:', event.payload.kind.text);
///         break;
///       case 'completed':
///         console.log('Done:', event.payload.kind.result);
///         break;
///     }
///   }
/// });
///
/// // Trigger task execution
/// const response = await invoke('run_agent_task_streaming', { id: 'task-123' });
/// console.log('Started task:', response.task_id);
/// ```
#[tauri::command]
pub async fn run_agent_task_streaming(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
) -> Result<StreamingTaskResponse, String> {
    // Check if task exists
    let task = state
        .core
        .storage
        .agent_tasks
        .get_task(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent task '{}' not found", id))?;

    // Check if already running
    let already_running = state.is_task_running(&id).await;
    if already_running {
        return Ok(StreamingTaskResponse {
            task_id: id,
            event_channel: TASK_STREAM_EVENT.to_string(),
            already_running: true,
        });
    }

    // Emit started event (emitter stored for future use in enhanced streaming)
    let _emitter = TauriEventEmitter::new(app_handle.clone());
    let execution_mode_str = match &task.execution_mode {
        ExecutionMode::Api => "api".to_string(),
        ExecutionMode::Cli(cfg) => format!("cli:{}", cfg.binary),
    };

    let started_event =
        TaskStreamEvent::started(&task.id, &task.name, &task.agent_id, &execution_mode_str);

    if let Err(e) = app_handle.emit(TASK_STREAM_EVENT, &started_event) {
        tracing::warn!("Failed to emit started event: {}", e);
    }

    // Trigger the task execution via runner (which will emit more events)
    if let Err(e) = state.run_task_now(id.clone()).await {
        // Emit failed event
        let failed_event = TaskStreamEvent::failed(&id, e.to_string(), 0, false);
        let _ = app_handle.emit(TASK_STREAM_EVENT, &failed_event);
        return Err(e.to_string());
    }

    Ok(StreamingTaskResponse {
        task_id: id,
        event_channel: TASK_STREAM_EVENT.to_string(),
        already_running: false,
    })
}

/// Information about an active/running task
#[derive(Debug, Clone, Serialize)]
pub struct ActiveTaskInfo {
    /// Task ID
    pub task_id: String,
    /// Task name
    pub task_name: String,
    /// Agent ID being executed
    pub agent_id: String,
    /// When the task started (milliseconds since epoch)
    pub started_at: i64,
    /// Execution mode
    pub execution_mode: String,
}

/// Get list of currently running/active agent tasks
///
/// Returns information about all tasks that are currently being executed.
#[tauri::command]
pub async fn get_active_agent_tasks(
    state: State<'_, AppState>,
) -> Result<Vec<ActiveTaskInfo>, String> {
    state.get_active_tasks().await.map_err(|e| e.to_string())
}

/// Emit a test event to verify the streaming system is working
///
/// This is useful for debugging and testing the event system from the frontend.
#[tauri::command]
pub async fn emit_test_task_event(
    app_handle: AppHandle,
    task_id: String,
    message: String,
) -> Result<(), String> {
    let event = TaskStreamEvent::output(&task_id, &message, false);
    app_handle
        .emit(TASK_STREAM_EVENT, &event)
        .map_err(|e| e.to_string())
}

/// Subscribe to task stream events
///
/// This is a no-op command that documents the event subscription pattern.
/// In Tauri v2, the frontend uses `listen()` to subscribe to events.
///
/// # Usage
///
/// ```typescript
/// import { listen } from '@tauri-apps/api/event';
/// import type { TaskStreamEvent } from './types/generated';
///
/// // Subscribe to all task events
/// const unlisten = await listen<TaskStreamEvent>('agent-task:stream', (event) => {
///   console.log('Received event:', event.payload);
/// });
///
/// // Later, unsubscribe
/// unlisten();
/// ```
#[tauri::command]
pub fn get_task_stream_event_name() -> String {
    TASK_STREAM_EVENT.to_string()
}

// ============================================================================
// Heartbeat Commands
// ============================================================================

use crate::agent_task::HEARTBEAT_EVENT;

/// Get the heartbeat event name for frontend subscription
///
/// The heartbeat events are now emitted inline by the AgentTaskRunner during
/// its poll cycle, so there's no separate heartbeat runner to manage.
///
/// # Usage
///
/// ```typescript
/// import { listen } from '@tauri-apps/api/event';
/// import type { HeartbeatEvent } from './types/generated';
///
/// const unlisten = await listen<HeartbeatEvent>('agent-task:heartbeat', (event) => {
///   console.log('Heartbeat:', event.payload);
/// });
/// ```
#[tauri::command]
pub fn get_heartbeat_event_name() -> String {
    HEARTBEAT_EVENT.to_string()
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
            TaskSchedule::Cron {
                expression,
                timezone,
            } => {
                assert_eq!(expression, "0 9 * * *");
                assert_eq!(timezone, Some("America/Los_Angeles".to_string()));
            }
            _ => panic!("Expected Cron schedule"),
        }
    }
}
