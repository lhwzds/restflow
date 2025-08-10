mod core;
mod engine;
mod storage;
use axum::{Json, Router, extract::State, routing::get, routing::post};
use core::workflow::Workflow;
use engine::executor::WorkflowExecutor;
use serde::Serialize;
use std::sync::Arc;
use storage::db::WorkflowStorage;

#[derive(Serialize)]
struct Health {
    status: String,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "restflow is working!".to_string(),
    })
}

async fn create_workflow(
    State(storage): State<Arc<WorkflowStorage>>,
    Json(workflow): Json<Workflow>,
) -> Json<serde_json::Value> {
    match storage.add_workflow(&workflow) {
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

async fn get_workflow(
    State(storage): State<Arc<WorkflowStorage>>,
    axum::extract::Path(id): axum::extract::Path<String>,
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

async fn execute_workflow(Json(workflow): Json<Workflow>) -> Json<serde_json::Value> {
    let executor = WorkflowExecutor::new(workflow);

    match executor.execute().await {
        Ok(result) => Json(result),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": e
        })),
    }
}

#[tokio::main]
async fn main() {
    let storage =
        Arc::new(WorkflowStorage::new("restflow.db").expect("Failed to initialize storage"));

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/workflow", post(create_workflow))
        .route("/api/workflow/:id", get(get_workflow))
        .route("/api/execute", post(execute_workflow))
        .with_state(storage);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("Failed to bind to port 3000");

    println!("Restflow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to run axum server");
}
