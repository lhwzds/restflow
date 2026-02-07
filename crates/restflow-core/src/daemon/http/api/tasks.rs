use crate::AppCore;
use crate::daemon::http::ApiError;
use crate::models::{
    AgentTask, AgentTaskStatus, BackgroundAgentControlAction, BackgroundAgentPatch,
    BackgroundAgentSpec, BackgroundMessage, BackgroundMessageSource, BackgroundProgress,
    MemoryConfig, MemoryScope, NotificationConfig, TaskSchedule,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query},
    routing::{get, patch, post, put},
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_tasks).post(create_task))
        .route("/{id}", get(get_task).delete(delete_task))
        .route("/{id}/pause", put(pause_task))
        .route("/{id}/resume", put(resume_task))
        .route("/background", post(create_background_agent))
        .route(
            "/background/{id}",
            patch(update_background_agent).delete(delete_background_agent),
        )
        .route("/background/{id}/control", post(control_background_agent))
        .route("/background/{id}/progress", get(get_background_progress))
        .route(
            "/background/{id}/messages",
            get(list_background_messages).post(send_background_message),
        )
}

#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    status: Option<String>,
}

async fn list_tasks(
    Extension(core): Extension<Arc<AppCore>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<Vec<AgentTask>>, ApiError> {
    let tasks = if let Some(status_str) = query.status {
        let status = parse_task_status(&status_str)?;
        core.storage.agent_tasks.list_tasks_by_status(status)?
    } else {
        core.storage.agent_tasks.list_tasks()?
    };
    Ok(Json(tasks))
}

async fn get_task(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<AgentTask>, ApiError> {
    let task = core
        .storage
        .agent_tasks
        .get_task(&id)?
        .ok_or_else(|| ApiError::not_found("Task"))?;
    Ok(Json(task))
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    name: String,
    agent_id: String,
    input: Option<String>,
    input_template: Option<String>,
    memory_scope: Option<MemoryScope>,
    cron: Option<String>,
}

async fn create_task(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<AgentTask>, ApiError> {
    let has_memory_scope = req.memory_scope.is_some();
    let schedule = match req.cron {
        Some(expression) => TaskSchedule::Cron {
            expression,
            timezone: None,
        },
        None => TaskSchedule::default(),
    };

    let mut task = core
        .storage
        .agent_tasks
        .create_task(req.name, req.agent_id, schedule)?;

    if let Some(input) = req.input {
        task.input = Some(input);
    }
    if let Some(input_template) = req.input_template {
        task.input_template = Some(input_template);
    }
    if let Some(memory_scope) = req.memory_scope {
        task.memory.memory_scope = memory_scope;
    }

    if task.input.is_some() || task.input_template.is_some() || has_memory_scope {
        core.storage.agent_tasks.update_task(&task)?;
    }

    Ok(Json(task))
}

async fn pause_task(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<AgentTask>, ApiError> {
    let task = core.storage.agent_tasks.pause_task(&id)?;
    Ok(Json(task))
}

async fn resume_task(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<AgentTask>, ApiError> {
    let task = core.storage.agent_tasks.resume_task(&id)?;
    Ok(Json(task))
}

async fn delete_task(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = core.storage.agent_tasks.delete_task(&id)?;
    Ok(Json(serde_json::json!({ "deleted": deleted, "id": id })))
}

fn parse_task_status(input: &str) -> Result<AgentTaskStatus, ApiError> {
    match input.trim().to_lowercase().as_str() {
        "active" => Ok(AgentTaskStatus::Active),
        "paused" => Ok(AgentTaskStatus::Paused),
        "running" => Ok(AgentTaskStatus::Running),
        "completed" => Ok(AgentTaskStatus::Completed),
        "failed" => Ok(AgentTaskStatus::Failed),
        _ => Err(ApiError::bad_request(format!("Unknown status: {}", input))),
    }
}

#[derive(Debug, Deserialize)]
struct CreateBackgroundAgentRequest {
    name: String,
    agent_id: String,
    schedule: TaskSchedule,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    input_template: Option<String>,
    #[serde(default)]
    notification: Option<NotificationConfig>,
    #[serde(default)]
    memory: Option<MemoryConfig>,
    #[serde(default)]
    memory_scope: Option<MemoryScope>,
}

async fn create_background_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateBackgroundAgentRequest>,
) -> Result<Json<AgentTask>, ApiError> {
    let task = core
        .storage
        .agent_tasks
        .create_background_agent(BackgroundAgentSpec {
            name: req.name,
            agent_id: req.agent_id,
            description: req.description,
            input: req.input,
            input_template: req.input_template,
            schedule: req.schedule,
            notification: req.notification,
            execution_mode: None,
            memory: merge_memory_scope(req.memory, req.memory_scope),
        })?;
    Ok(Json(task))
}

#[derive(Debug, Deserialize)]
struct UpdateBackgroundAgentRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    input_template: Option<String>,
    #[serde(default)]
    schedule: Option<TaskSchedule>,
    #[serde(default)]
    notification: Option<NotificationConfig>,
    #[serde(default)]
    memory: Option<MemoryConfig>,
    #[serde(default)]
    memory_scope: Option<MemoryScope>,
}

async fn update_background_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateBackgroundAgentRequest>,
) -> Result<Json<AgentTask>, ApiError> {
    let task = core.storage.agent_tasks.update_background_agent(
        &id,
        BackgroundAgentPatch {
            name: req.name,
            description: req.description,
            agent_id: req.agent_id,
            input: req.input,
            input_template: req.input_template,
            schedule: req.schedule,
            notification: req.notification,
            execution_mode: None,
            memory: merge_memory_scope(req.memory, req.memory_scope),
        },
    )?;
    Ok(Json(task))
}

fn merge_memory_scope(
    memory: Option<MemoryConfig>,
    memory_scope: Option<MemoryScope>,
) -> Option<MemoryConfig> {
    match (memory, memory_scope) {
        (Some(mut memory), Some(scope)) => {
            memory.memory_scope = scope;
            Some(memory)
        }
        (Some(memory), None) => Some(memory),
        (None, Some(scope)) => Some(MemoryConfig {
            memory_scope: scope,
            ..MemoryConfig::default()
        }),
        (None, None) => None,
    }
}

async fn delete_background_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = core.storage.agent_tasks.delete_task(&id)?;
    Ok(Json(serde_json::json!({ "deleted": deleted, "id": id })))
}

#[derive(Debug, Deserialize)]
struct BackgroundControlRequest {
    action: BackgroundAgentControlAction,
}

async fn control_background_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(req): Json<BackgroundControlRequest>,
) -> Result<Json<AgentTask>, ApiError> {
    let task = core
        .storage
        .agent_tasks
        .control_background_agent(&id, req.action)?;
    Ok(Json(task))
}

#[derive(Debug, Deserialize)]
struct BackgroundProgressQuery {
    #[serde(default = "default_progress_limit")]
    event_limit: usize,
}

fn default_progress_limit() -> usize {
    10
}

async fn get_background_progress(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Query(query): Query<BackgroundProgressQuery>,
) -> Result<Json<BackgroundProgress>, ApiError> {
    let progress = core
        .storage
        .agent_tasks
        .get_background_agent_progress(&id, query.event_limit)?;
    Ok(Json(progress))
}

#[derive(Debug, Deserialize)]
struct SendBackgroundMessageRequest {
    message: String,
    #[serde(default)]
    source: Option<BackgroundMessageSource>,
}

async fn send_background_message(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(req): Json<SendBackgroundMessageRequest>,
) -> Result<Json<BackgroundMessage>, ApiError> {
    let message = core.storage.agent_tasks.send_background_agent_message(
        &id,
        req.message,
        req.source.unwrap_or(BackgroundMessageSource::User),
    )?;
    Ok(Json(message))
}

#[derive(Debug, Deserialize)]
struct ListBackgroundMessagesQuery {
    #[serde(default = "default_message_limit")]
    limit: usize,
}

fn default_message_limit() -> usize {
    50
}

async fn list_background_messages(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Query(query): Query<ListBackgroundMessagesQuery>,
) -> Result<Json<Vec<BackgroundMessage>>, ApiError> {
    let messages = core
        .storage
        .agent_tasks
        .list_background_agent_messages(&id, query.limit.max(1))?;
    Ok(Json(messages))
}
