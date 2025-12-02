use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use restflow_workflow::models::{ExecutionHistoryPage, Workflow};
use restflow_workflow::services::workflow as workflow_service;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info};

#[derive(Serialize, Deserialize)]
pub struct WorkflowIdResponse {
    pub id: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExecutionResponse {
    pub execution_id: String,
    pub workflow_id: String,
}

#[derive(Deserialize)]
pub struct ExecutionHistoryQuery {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

pub async fn list_workflows(State(state): State<AppState>) -> Json<ApiResponse<Vec<Workflow>>> {
    match state.storage.workflows.list_workflows() {
        Ok(workflows) => Json(ApiResponse::ok(workflows)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to list workflows: {}",
            e
        ))),
    }
}

pub async fn create_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<Workflow>,
) -> Json<ApiResponse<WorkflowIdResponse>> {
    match state.storage.workflows.create_workflow(&workflow) {
        Ok(_) => Json(ApiResponse::ok_with_message(
            WorkflowIdResponse {
                id: workflow.id.clone(),
            },
            format!("Workflow {} saved!", workflow.name),
        )),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to save workflow: {}",
            e
        ))),
    }
}

pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<Workflow>> {
    match state.storage.workflows.get_workflow(&id) {
        Ok(workflow) => Json(ApiResponse::ok(workflow)),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

pub async fn update_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(workflow): Json<Workflow>,
) -> Json<ApiResponse<()>> {
    match state.storage.workflows.update_workflow(&id, &workflow) {
        Ok(_) => Json(ApiResponse::message(format!("Workflow {} updated!", id))),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to update workflow: {}",
            e
        ))),
    }
}

pub async fn delete_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.storage.workflows.delete_workflow(&id) {
        Ok(_) => Json(ApiResponse::message(format!("Workflow {} deleted!", id))),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to delete workflow: {}",
            e
        ))),
    }
}

pub async fn execute_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<Workflow>,
) -> Json<ApiResponse<Value>> {
    let workflow_id = workflow.id.clone();
    let workflow_name = workflow.name.clone();

    info!(
        workflow_id = %workflow_id,
        workflow_name = %workflow_name,
        "Executing inline workflow"
    );

    match workflow_service::execute_workflow_inline(&state, workflow).await {
        Ok(output) => {
            info!(
                workflow_id = %workflow_id,
                workflow_name = %workflow_name,
                "Inline workflow completed successfully"
            );
            Json(ApiResponse::ok(output))
        }
        Err(e) => {
            error!(
                workflow_id = %workflow_id,
                workflow_name = %workflow_name,
                error = %e,
                "Workflow execution failed"
            );
            Json(ApiResponse::error(format!(
                "Workflow execution failed: {}",
                e
            )))
        }
    }
}

// execute_workflow_by_id removed - full workflow execution uses async submissions

pub async fn submit_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<ApiResponse<ExecutionResponse>> {
    match state.executor.submit(workflow_id.clone(), input).await {
        Ok(execution_id) => Json(ApiResponse::ok(ExecutionResponse {
            execution_id,
            workflow_id,
        })),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to submit workflow: {}",
            e
        ))),
    }
}

pub async fn list_workflow_executions(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Query(params): Query<ExecutionHistoryQuery>,
) -> Json<ApiResponse<ExecutionHistoryPage>> {
    let (page, page_size) = if let Some(limit) = params.limit {
        (1usize, limit)
    } else {
        (params.page, params.page_size)
    };

    match restflow_workflow::services::task::list_execution_history(
        &state,
        &workflow_id,
        page,
        page_size,
    )
    .await
    {
        Ok(executions) => Json(ApiResponse::ok(executions)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to list execution history: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_workflow::AppCore;
    use restflow_workflow::models::{Node, NodeType};
    use std::sync::Arc;
    use tempfile::{TempDir, tempdir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    fn create_test_workflow(id: &str) -> Workflow {
        Workflow {
            id: id.to_string(),
            name: format!("Test Workflow {}", id),
            nodes: vec![Node {
                id: "node1".to_string(),
                node_type: NodeType::Agent,
                config: serde_json::json!({
                    "type": "Agent",
                    "data": {
                        "model": "gpt-4",
                        "prompt": "Test prompt"
                    }
                }),
                position: None,
            }],
            edges: vec![],
        }
    }

    #[tokio::test]
    async fn test_list_workflows_empty() {
        let (app, _tmp_dir) = create_test_app().await;

        let response = list_workflows(State(app)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.data.is_some());
        assert_eq!(body.data.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_create_workflow() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow("wf-001");

        let response = create_workflow(State(app.clone()), Json(workflow.clone())).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.message.is_some());
        assert!(body.message.unwrap().contains("saved"));

        let data = body.data.unwrap();
        assert_eq!(data.id, "wf-001");
    }

    #[tokio::test]
    async fn test_get_workflow() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow("wf-001");

        let _ = create_workflow(State(app.clone()), Json(workflow)).await;

        let response = get_workflow(State(app), Path("wf-001".to_string())).await;
        let body = response.0;

        assert!(body.success);
        let data = body.data.unwrap();
        assert_eq!(data.id, "wf-001");
        assert_eq!(data.name, "Test Workflow wf-001");
    }

    #[tokio::test]
    async fn test_get_nonexistent_workflow() {
        let (app, _tmp_dir) = create_test_app().await;

        let response = get_workflow(State(app), Path("nonexistent".to_string())).await;
        let body = response.0;

        assert!(!body.success);
        assert!(body.message.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_workflow() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow("wf-001");

        let _ = create_workflow(State(app.clone()), Json(workflow.clone())).await;

        let mut updated = workflow;
        updated.name = "Updated Name".to_string();

        let response = update_workflow(State(app), Path("wf-001".to_string()), Json(updated)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.message.unwrap().contains("updated"));
    }

    #[tokio::test]
    async fn test_delete_workflow() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow("wf-001");

        let _ = create_workflow(State(app.clone()), Json(workflow)).await;

        let response = delete_workflow(State(app.clone()), Path("wf-001".to_string())).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.message.unwrap().contains("deleted"));

        let get_response = get_workflow(State(app), Path("wf-001".to_string())).await;
        assert!(!get_response.0.success);
    }
}
