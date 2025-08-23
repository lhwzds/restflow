mod api_response;
mod core;
mod engine;
mod node;
mod static_assets;
mod storage;
mod tools;
use api_response::{error, not_found, success, success_with_message};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{Method, header},
    routing::{delete, get, post, put},
};
use core::workflow::Workflow;
use engine::executor::{AsyncWorkflowExecutor, WorkflowExecutor};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use storage::{Storage, TaskStatus, SystemConfig};
use tower_http::cors::CorsLayer;

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
async fn list_workflows(State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>) -> Json<serde_json::Value> {
    storage.workflows.list_workflows()
        .map(success)
        .unwrap_or_else(|e| error(format!("Failed to list workflows: {}", e)))
}

// Create a new workflow
// POST /api/workflow/create
// Body: JSON workflow object
async fn create_workflow(
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Json(workflow): Json<Workflow>,
) -> Json<serde_json::Value> {
    storage.workflows.create_workflow(&workflow)
        .map(|_| success_with_message(
            serde_json::json!({"id": workflow.id}),
            format!("Workflow {} saved!", workflow.name)
        ))
        .unwrap_or_else(|e| error(format!("Failed to save workflow: {}", e)))
}

// Get workflow by ID
// GET /api/workflow/get/{id}
async fn get_workflow(
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    storage.workflows.get_workflow(&id)
        .map(|opt| opt
            .map(|w| Json(serde_json::json!(w)))
            .unwrap_or_else(|| not_found("Workflow not found".to_string())))
        .unwrap_or_else(|e| error(format!("Failed to get workflow: {}", e)))
}

// Update existing workflow
// PUT /api/workflow/update/{id}
// Body: JSON workflow object
async fn update_workflow(
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(id): Path<String>,
    Json(workflow): Json<Workflow>,
) -> Json<serde_json::Value> {
    storage.workflows.update_workflow(&id, &workflow)
        .map(|_| success_with_message(
            serde_json::json!({}),
            format!("Workflow {} updated!", id)
        ))
        .unwrap_or_else(|e| error(format!("Failed to update workflow: {}", e)))
}

// Delete workflow by ID
// DELETE /api/workflow/delete/{id}
async fn delete_workflow(
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    storage.workflows.delete_workflow(&id)
        .map(|_| success_with_message(
            serde_json::json!({}),
            format!("Workflow {} deleted!", id)
        ))
        .unwrap_or_else(|e| error(format!("Failed to delete workflow: {}", e)))
}

// Execute workflow directly
// POST /api/workflow/execute
// Body: JSON workflow object
async fn execute_workflow(Json(workflow): Json<Workflow>) -> Json<serde_json::Value> {
    let mut executor = WorkflowExecutor::new(workflow);

    match executor.execute().await {
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
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match storage.workflows.get_workflow(&id) {
        Ok(Some(workflow)) => {
            let mut executor = WorkflowExecutor::new(workflow);
            match executor.execute().await {
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

// POST /api/workflow/submit/{id}
#[derive(Deserialize)]
struct SubmitRequest {
    #[serde(default = "default_input")]
    input: serde_json::Value,
}

fn default_input() -> serde_json::Value {
    serde_json::json!({})
}

async fn submit_workflow(
    State((storage, executor)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(id): Path<String>,
    Json(req): Json<SubmitRequest>,
) -> Json<serde_json::Value> {
    match storage.workflows.get_workflow(&id) {
        Ok(Some(_)) => {
            match executor.submit(id, req.input).await {
                Ok(execution_id) => Json(serde_json::json!({
                    "status": "success",
                    "execution_id": execution_id,
                    "message": "Workflow submitted to queue"
                })),
                Err(e) => Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Failed to submit workflow: {}", e)
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

// GET /api/execution/{id}
async fn get_execution_status(
    State((_, executor)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(execution_id): Path<String>,
) -> Json<serde_json::Value> {
    match executor.get_execution_status(&execution_id).await {
        Ok(tasks) => {
            // Calculate overall execution status
            let has_failed = tasks.iter().any(|t| t.status == TaskStatus::Failed);
            let has_running = tasks.iter().any(|t| t.status == TaskStatus::Running);
            let has_pending = tasks.iter().any(|t| t.status == TaskStatus::Pending);
            let all_completed = tasks.iter().all(|t| t.status == TaskStatus::Completed);
            
            let execution_status = if has_failed {
                "failed"
            } else if has_running {
                "running"
            } else if has_pending {
                "pending"
            } else if all_completed {
                "completed"
            } else {
                "unknown"
            };
            
            Json(serde_json::json!({
                "status": "success",
                "execution_id": execution_id,
                "execution_status": execution_status,
                "tasks": tasks,
                "task_count": tasks.len()
            }))
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get execution: {}", e)
        })),
    }
}

// GET /api/task/{id}
async fn get_task_status(
    State((_, executor)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Path(task_id): Path<String>,
) -> Json<serde_json::Value> {
    match executor.get_task_status(&task_id).await {
        Ok(Some(task)) => Json(serde_json::json!(task)),
        Ok(None) => Json(serde_json::json!({
            "status": "error",
            "message": "Task not found"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get task: {}", e)
        })),
    }
}

// GET /api/tasks?workflow_id=xxx&status=xxx
#[derive(Deserialize)]
struct ListTasksQuery {
    workflow_id: Option<String>,
    status: Option<String>,
}

// GET /api/config
async fn get_config(
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
) -> Json<serde_json::Value> {
    match storage.config.get_config() {
        Ok(Some(config)) => success(serde_json::json!(config)),
        Ok(None) => success(serde_json::json!(SystemConfig::default())),
        Err(e) => error(format!("Failed to get config: {}", e)),
    }
}

// PUT /api/config
async fn update_config(
    State((storage, _)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    Json(config): Json<SystemConfig>,
) -> Json<serde_json::Value> {
    match storage.config.update_config(config) {
        Ok(_) => success_with_message(
            serde_json::json!({"status": "success"}),
            "Configuration updated successfully. Restart required for worker count changes.".to_string()
        ),
        Err(e) => error(format!("Failed to update config: {}", e)),
    }
}

async fn list_tasks(
    State((_, executor)): State<(Arc<Storage>, Arc<AsyncWorkflowExecutor>)>,
    axum::extract::Query(query): axum::extract::Query<ListTasksQuery>,
) -> Json<serde_json::Value> {
    let status = query.status.and_then(|s| match s.as_str() {
        "pending" => Some(TaskStatus::Pending),
        "running" => Some(TaskStatus::Running),
        "completed" => Some(TaskStatus::Completed),
        "failed" => Some(TaskStatus::Failed),
        _ => None,
    });
    
    match executor.list_tasks(query.workflow_id.as_deref(), status).await {
        Ok(tasks) => Json(serde_json::json!({
            "status": "success",
            "data": tasks
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to list tasks: {}", e)
        })),
    }
}


#[tokio::main]
async fn main() {
    let storage =
        Arc::new(Storage::new("restflow.db").expect("Failed to initialize storage"));
    
    // Get worker count from database configuration
    let num_workers = storage.config.get_worker_count()
        .unwrap_or(4);
    
    println!("Starting RestFlow with {} workers (from config)", num_workers);
    
    let async_executor = Arc::new(AsyncWorkflowExecutor::with_workers(storage.clone(), num_workers));
    async_executor.start().await;

    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:3000".parse().unwrap(),
            "http://localhost:5173".parse().unwrap(),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true);

    let shared_state = (storage.clone(), async_executor);
    
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
        
        // Query single task 
        .route("/api/task/status/{task_id}", get(get_task_status))                
        .route("/api/task/list", get(list_tasks))
        
        // System configuration
        .route("/api/config", get(get_config).put(update_config))
        
        .fallback(static_assets::static_handler)
        .layer(cors)
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    println!("RestFlow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to run axum server");
}
