mod core;
mod engine;
use axum::{Json, Router, routing::get, routing::post};
use core::workflow::Workflow;
use engine::executor::WorkflowExecutor;
use serde::Serialize;

#[derive(Serialize)]
struct Health {
    status: String,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "restflow is working!".to_string(),
    })
}

async fn create_workflow(Json(workflow): Json<Workflow>) -> String {
    format!("Hello new workflow {}!", workflow.name)
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
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/workflow", post(create_workflow))
        .route("/api/execute", post(execute_workflow));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("Failed to bind to port 3000");

    println!("Restflow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to run axum server");
}
