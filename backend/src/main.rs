mod core;
mod engine;
mod node;
mod storage;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post, put},
};
use core::workflow::Workflow;
use engine::executor::WorkflowExecutor;
use serde::Serialize;
use std::sync::Arc;
use storage::db::WorkflowStorage;
use tower_http::cors::{CorsLayer, Any};

#[derive(Serialize)]
struct Health {
    status: String,
}

// Health check endpoint
async fn health() -> Json<Health> {
    Json(Health {
        status: "restflow is working!".to_string(),
    })
}

// List all workflows
// GET /api/workflow/list
async fn list_workflows(State(storage): State<Arc<WorkflowStorage>>) -> Json<serde_json::Value> {
    match storage.list_workflows() {
        Ok(workflows) => Json(serde_json::json!({
            "status": "success",
            "data": workflows
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to list workflows: {}", e)
        })),
    }
}

// Create a new workflow
// POST /api/workflow/create
// Body: JSON workflow object
async fn create_workflow(
    State(storage): State<Arc<WorkflowStorage>>,
    Json(workflow): Json<Workflow>,
) -> Json<serde_json::Value> {
    match storage.create_workflow(&workflow) {
        Ok(_) => Json(serde_json::json!({
              "status": "success",
              "message": format!("Workflow {} saved!", workflow.name),
              "id": workflow.id
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to save workflow: {}", e)
        })),
    }
}

// Get workflow by ID
// GET /api/workflow/get/{id}
async fn get_workflow(
    State(storage): State<Arc<WorkflowStorage>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match storage.get_workflow(&id) {
        Ok(Some(workflow)) => Json(serde_json::json!(workflow)),
        Ok(None) => Json(serde_json::json!({
            "status": "error",
            "message": "Workflow not found"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get workflow: {}", e)
        })),
    }
}

// Update existing workflow
// PUT /api/workflow/update/{id}
// Body: JSON workflow object
async fn update_workflow(
    State(storage): State<Arc<WorkflowStorage>>,
    Path(id): Path<String>,
    Json(workflow): Json<Workflow>,
) -> Json<serde_json::Value> {
    match storage.update_workflow(&id, &workflow) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": format!("Workflow {} updated!", id)
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to update workflow: {}", e)
        })),
    }
}

// Delete workflow by ID
// DELETE /api/workflow/delete/{id}
async fn delete_workflow(
    State(storage): State<Arc<WorkflowStorage>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match storage.delete_workflow(&id) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": format!("Workflow {} deleted!", id)
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to delete workflow: {}", e)
        })),
    }
}

// Execute workflow directly
// POST /api/workflow/execute
// Body: JSON workflow object
async fn execute_workflow(Json(workflow): Json<Workflow>) -> Json<serde_json::Value> {
    let executor = WorkflowExecutor::new(workflow);

    match executor.execute_workflow().await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": e
        })),
    }
}

// Execute workflow by ID
// POST /api/workflow/execute/{id}
async fn execute_workflow_by_id(
    State(storage): State<Arc<WorkflowStorage>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match storage.get_workflow(&id) {
        Ok(Some(workflow)) => {
            let executor = WorkflowExecutor::new(workflow);
            match executor.execute_workflow().await {
                Ok(result) => Json(result),
                Err(e) => Json(serde_json::json!({
                    "status": "error",
                    "message": e
                })),
            }
        }
        Ok(None) => Json(serde_json::json!({
            "status": "error",
            "message": "Workflow not found"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get workflow: {}", e)
        })),
    }
}

#[tokio::main]
async fn main() {
    let storage =
        Arc::new(WorkflowStorage::new("restflow.db").expect("Failed to initialize storage"));

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/workflow/list", get(list_workflows))
        .route("/api/workflow/create", post(create_workflow))
        .route("/api/workflow/get/{id}", get(get_workflow))
        .route("/api/workflow/update/{id}", put(update_workflow))
        .route("/api/workflow/delete/{id}", delete(delete_workflow))
        .route("/api/workflow/execute", post(execute_workflow))
        .route("/api/workflow/execute/{id}", post(execute_workflow_by_id))
        .layer(cors)
        .with_state(storage);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    println!("RestFlow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to run axum server");
}
