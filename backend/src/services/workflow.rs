use crate::{AppCore, models::Workflow, engine::executor::WorkflowExecutor};
use serde_json::Value;
use std::sync::Arc;

// Core workflow functions that can be used by both Axum and Tauri

pub async fn list_workflows(core: &Arc<AppCore>) -> Result<Vec<Workflow>, String> {
    core.storage.workflows.list_workflows()
        .map_err(|e| format!("Failed to list workflows: {}", e))
}

pub async fn get_workflow(core: &Arc<AppCore>, id: &str) -> Result<Workflow, String> {
    core.storage.workflows.get_workflow(id)
        .map_err(|e| e.to_string())
}

pub async fn create_workflow(core: &Arc<AppCore>, mut workflow: Workflow) -> Result<Workflow, String> {
    // Generate ID if not provided
    if workflow.id.is_empty() {
        workflow.id = format!("wf_{}", uuid::Uuid::new_v4());
    }
    
    core.storage.workflows.create_workflow(&workflow)
        .map_err(|e| format!("Failed to save workflow: {}", e))?;
    
    Ok(workflow)
}

pub async fn update_workflow(core: &Arc<AppCore>, id: &str, workflow: Workflow) -> Result<Workflow, String> {
    core.storage.workflows.update_workflow(id, &workflow)
        .map_err(|e| format!("Failed to update workflow: {}", e))?;
    
    Ok(workflow)
}

pub async fn delete_workflow(core: &Arc<AppCore>, id: &str) -> Result<(), String> {
    // Try to deactivate any active triggers for this workflow
    let _ = core.trigger_manager.deactivate_workflow(id).await;
    
    core.storage.workflows.delete_workflow(id)
        .map_err(|e| format!("Failed to delete workflow: {}", e))
}

// Execution functions (also in workflows API)

pub async fn execute_workflow_inline(mut workflow: Workflow) -> Result<Value, String> {
    workflow.id = format!("inline-{}", uuid::Uuid::new_v4());
    
    let mut wf_executor = WorkflowExecutor::new_sync(workflow);
    wf_executor.execute().await
        .map_err(|e| format!("Workflow execution failed: {}", e))
}

pub async fn execute_workflow_by_id(
    core: &Arc<AppCore>, 
    workflow_id: &str, 
    input: Value
) -> Result<Value, String> {
    let workflow = core.storage.workflows.get_workflow(workflow_id)
        .map_err(|e| e.to_string())?;
    
    let mut wf_executor = WorkflowExecutor::new_sync(workflow);
    wf_executor.set_input(input);
    wf_executor.execute().await
        .map_err(|e| format!("Workflow execution failed: {}", e))
}

pub async fn submit_workflow(
    core: &Arc<AppCore>,
    workflow_id: &str,
    input: Value
) -> Result<String, String> {
    core.executor.submit(workflow_id.to_string(), input).await
        .map_err(|e| format!("Failed to submit workflow: {}", e))
}

pub async fn get_execution_status(
    core: &Arc<AppCore>,
    execution_id: &str
) -> Result<Vec<crate::models::Task>, String> {
    core.executor.get_execution_status(execution_id).await
        .map_err(|e| format!("Failed to get execution status: {}", e))
}

