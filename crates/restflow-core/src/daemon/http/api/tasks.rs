use crate::AppCore;
use crate::daemon::http::ApiError;
use crate::models::{AgentTask, AgentTaskStatus, TaskSchedule};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query},
    routing::{get, put},
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_tasks).post(create_task))
        .route("/{id}", get(get_task).delete(delete_task))
        .route("/{id}/pause", put(pause_task))
        .route("/{id}/resume", put(resume_task))
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
    cron: Option<String>,
}

async fn create_task(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<AgentTask>, ApiError> {
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
