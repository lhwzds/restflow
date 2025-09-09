mod static_assets;

use backend::{AppCore, api};
use std::sync::Arc;
use api::{
    workflows::*, triggers::*, tasks::*, config::*, python::*
};
use axum::{
    Router,
    http::{Method, header},
    routing::{delete, get, post, put},
};
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
    // Use AppCore for unified initialization
    let core = Arc::new(
        AppCore::new("restflow.db")
            .await
            .expect("Failed to initialize app core")
    );
    
    println!("Starting RestFlow server");
    
    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(["http://localhost:5173".parse().unwrap()])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true);

    // AppState is now just an alias for Arc<AppCore>
    let shared_state = core.clone();
    
    let app = Router::new()
        .route("/health", get(health))
        
        // Workflow management 
        .route("/api/workflow/list", get(list_workflows))
        .route("/api/workflow/create", post(create_workflow))
        .route("/api/workflow/get/{id}", get(get_workflow))
        .route("/api/workflow/update/{id}", put(update_workflow))
        .route("/api/workflow/delete/{id}", delete(delete_workflow))
        
        // Execution operations
        .route("/api/execution/sync/run", post(execute_workflow))              
        .route("/api/execution/sync/run-workflow/{workflow_id}", post(execute_workflow_by_id))  
        .route("/api/execution/async/submit/{workflow_id}", post(submit_workflow)) 
        .route("/api/execution/status/{execution_id}", get(get_execution_status)) 
        
        // Task operations
        .route("/api/task/status/{task_id}", get(get_task_status))                
        .route("/api/task/list", get(list_tasks))
        .route("/api/node/execute", post(execute_node))
        
        // System configuration
        .route("/api/config", get(get_config).put(update_config))
        
        // Trigger management
        .route("/api/workflow/{id}/activate", put(activate_workflow))
        .route("/api/workflow/{id}/deactivate", put(deactivate_workflow))
        .route("/api/workflow/{id}/trigger-status", get(get_workflow_trigger_status))
        .route("/api/workflow/{id}/test", post(test_workflow_trigger))
        
        // Webhook endpoint (accepts any HTTP method)
        .route("/api/triggers/webhook/{webhook_id}", 
            get(handle_webhook_trigger)
            .post(handle_webhook_trigger)
            .put(handle_webhook_trigger)
            .delete(handle_webhook_trigger)
            .patch(handle_webhook_trigger))
        
        // Python integration endpoints (simplified for internal use)
        .route("/api/python/execute", post(execute_script))
        .route("/api/python/scripts", get(list_scripts))
        
        .fallback(static_assets::static_handler)
        .layer(cors)
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    println!("RestFlow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}