use axum::{Json, Router, routing::get, routing::post};

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct Health {
    status: String,
}

#[derive(Serialize, Deserialize)]
struct Workflow {
    name: String,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "restflow is working!".to_string(),
    })
}

async fn create_workflow(Json(workflow): Json<Workflow>) -> String {
    format!("Hello new workflow {}!", workflow.name)
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/workflow", post(create_workflow));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    println!("Restflow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to run axum server");
}
