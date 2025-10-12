mod static_assets;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod api;

use api::{agents::*, config::*, python::*, secrets::*, tasks::*, triggers::*, workflows::*};
use axum::{
    Router,
    http::{Method, header},
    routing::{get, post, put},
};
use restflow_core::{AppCore, paths};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[derive(serde::Serialize)]
struct Health {
    status: String,
}

async fn health() -> axum::Json<Health> {
    axum::Json(Health {
        status: "restflow is working!".to_string(),
    })
}

#[tokio::main]
async fn main() {
    // Initialize tracing logger
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,restflow_server=debug".into()),
        )
        .with_target(false)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    tracing::info!("Starting RestFlow backend server");

    // Use AppCore for unified initialization
    let db_path =
        paths::ensure_database_path_string().expect("Failed to determine RestFlow database path");
    let core = Arc::new(
        AppCore::new(&db_path)
            .await
            .expect("Failed to initialize app core"),
    );

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    // AppState is now just an alias for Arc<AppCore>
    let shared_state = core.clone();

    let app = Router::new()
        .route("/health", get(health))
        // Workflow management (RESTful)
        .route("/api/workflows", get(list_workflows).post(create_workflow))
        .route(
            "/api/workflows/{id}",
            get(get_workflow)
                .put(update_workflow)
                .delete(delete_workflow),
        )
        // Execution operations (RESTful)
        .route("/api/workflows/execute", post(execute_workflow))
        .route(
            "/api/workflows/{workflow_id}/execute",
            post(execute_workflow_by_id),
        )
        .route(
            "/api/workflows/{workflow_id}/executions",
            post(submit_workflow),
        )
        .route("/api/executions/{execution_id}", get(get_execution_status))
        // Task operations (RESTful)
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks/{task_id}", get(get_task_status))
        .route("/api/nodes/execute", post(execute_node))
        // System configuration
        .route("/api/config", get(get_config).put(update_config))
        // Trigger management
        .route("/api/workflows/{id}/activate", put(activate_workflow))
        .route("/api/workflows/{id}/deactivate", put(deactivate_workflow))
        .route(
            "/api/workflows/{id}/trigger-status",
            get(get_workflow_trigger_status),
        )
        .route("/api/workflows/{id}/test", post(test_workflow_trigger))
        // Webhook endpoint (accepts any HTTP method)
        .route(
            "/api/triggers/webhook/{webhook_id}",
            get(handle_webhook_trigger)
                .post(handle_webhook_trigger)
                .put(handle_webhook_trigger)
                .delete(handle_webhook_trigger)
                .patch(handle_webhook_trigger),
        )
        // Python integration endpoints (simplified for internal use)
        .route("/api/python/execute", post(execute_script))
        .route("/api/python/scripts", get(list_scripts))
        // Agent management endpoints
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
        .route("/api/agents/{id}/execute", post(execute_agent))
        .route("/api/agents/execute-inline", post(execute_agent_inline))
        // Secret management endpoints
        .route("/api/secrets", get(list_secrets).post(create_secret))
        .route(
            "/api/secrets/{key}",
            put(update_secret).delete(delete_secret),
        )
        .fallback(static_assets::static_handler)
        .layer(cors)
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    tracing::info!("RestFlow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}
