use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use restflow_workflow::models::{Node, NodeType, Task, TaskStatus};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TaskListQuery {
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct ExecuteNodeResponse {
    pub task_id: String,
    pub message: String,
}

pub async fn get_execution_status(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
) -> Json<ApiResponse<Vec<Task>>> {
    match state.executor.get_execution_status(&execution_id).await {
        Ok(tasks) => Json(ApiResponse::ok(tasks)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to get execution status: {}",
            e
        ))),
    }
}

pub async fn get_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Json<ApiResponse<Task>> {
    match state.executor.get_task_status(&task_id).await {
        Ok(task) => Json(ApiResponse::ok(task)),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<TaskListQuery>,
) -> Json<ApiResponse<Vec<Task>>> {
    let _limit = params.limit.unwrap_or(100) as usize;
    let status_filter = params.status.clone();

    match state.executor.list_tasks(None, status_filter).await {
        Ok(tasks) => {
            let filtered = if let Some(exec_id) = params.execution_id {
                tasks
                    .into_iter()
                    .filter(|t| t.execution_id == exec_id)
                    .collect::<Vec<_>>()
            } else {
                tasks
            };

            Json(ApiResponse::ok(filtered))
        }
        Err(e) => Json(ApiResponse::error(format!("Failed to list tasks: {}", e))),
    }
}

pub async fn execute_node(
    State(state): State<AppState>,
    Json(node): Json<Node>,
) -> Json<ApiResponse<ExecuteNodeResponse>> {
    if node.node_type == NodeType::Python
        && let Err(e) = state.get_python_manager().await
    {
        return Json(ApiResponse::error(format!(
            "Failed to initialize Python manager: {}",
            e
        )));
    }

    match state
        .executor
        .submit_node(node, serde_json::json!({}))
        .await
    {
        Ok(task_id) => Json(ApiResponse::ok(ExecuteNodeResponse {
            task_id,
            message: "Node execution started".to_string(),
        })),
        Err(e) => Json(ApiResponse::error(format!("Failed to execute node: {}", e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_workflow::AppCore;
    use restflow_workflow::models::{NodeType, Workflow};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::{TempDir, tempdir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    fn create_test_workflow() -> Workflow {
        Workflow {
            id: "test-workflow".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![Node {
                id: "node1".to_string(),
                node_type: NodeType::Agent,
                config: serde_json::json!({
                    "type": "Agent",
                    "data": {
                        "model": "claude-sonnet-4-5",
                        "prompt": "Test"
                    }
                }),
                position: None,
            }],
            edges: vec![],
        }
    }

    async fn wait_for_execution_tasks(app: &Arc<AppCore>, execution_id: &str) -> Vec<Task> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let tasks = app
                .executor
                .get_execution_status(execution_id)
                .await
                .expect("Failed to query execution status");

            if !tasks.is_empty() {
                return tasks;
            }

            if Instant::now() >= deadline {
                panic!(
                    "Execution {} did not produce tasks within expected time",
                    execution_id
                );
            }

            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn wait_for_task_visibility(app: &Arc<AppCore>, task_id: &str) -> Task {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match app.executor.get_task_status(task_id).await {
                Ok(task) => return task,
                Err(e) => {
                    if Instant::now() >= deadline {
                        panic!("Task {} not visible within expected time: {}", task_id, e);
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        }
    }

    #[tokio::test]
    async fn test_list_tasks_empty() {
        let (app, _tmp_dir) = create_test_app().await;

        let response = list_tasks(
            State(app),
            Query(TaskListQuery {
                execution_id: None,
                status: None,
                limit: None,
            }),
        )
        .await;
        let body = response.0;

        assert!(body.success);
        assert_eq!(body.data.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_execute_node() {
        let (app, _tmp_dir) = create_test_app().await;

        let node = Node {
            id: "test-node".to_string(),
            node_type: NodeType::Agent,
            config: serde_json::json!({
                "type": "Agent",
                "data": {
                    "model": "claude-sonnet-4-5",
                    "prompt": "Test"
                }
            }),
            position: None,
        };

        let response = execute_node(State(app), Json(node)).await;
        let body = response.0;

        assert!(body.success);
        let data = body.data.unwrap();
        assert!(!data.task_id.is_empty());
        assert!(data.message.contains("started"));
    }

    #[tokio::test]
    async fn test_get_task_status() {
        let (app, _tmp_dir) = create_test_app().await;

        let node = Node {
            id: "test-node".to_string(),
            node_type: NodeType::Agent,
            config: serde_json::json!({
                "type": "Agent",
                "data": {
                    "model": "claude-sonnet-4-5",
                    "prompt": "Test"
                }
            }),
            position: None,
        };

        let exec_response = execute_node(State(app.clone()), Json(node)).await;
        let task_id = exec_response.0.data.unwrap().task_id;

        let task = wait_for_task_visibility(&app, &task_id).await;
        assert_eq!(task.id, task_id);
        assert_eq!(task.node_id, "test-node");
    }

    #[tokio::test]
    async fn test_get_execution_status() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow();

        app.storage.workflows.create_workflow(&workflow).unwrap();

        let execution_id = app
            .executor
            .submit("test-workflow".to_string(), serde_json::json!({}))
            .await
            .expect("Failed to submit workflow asynchronously");

        let tasks = wait_for_execution_tasks(&app, &execution_id).await;
        assert!(!tasks.is_empty());
    }
}
