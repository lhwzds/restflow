use crate::{AppCore, engine::executor::WorkflowExecutor, models::Workflow};
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;

// Core workflow functions that can be used by both Axum and Tauri

pub async fn list_workflows(core: &Arc<AppCore>) -> Result<Vec<Workflow>> {
    core.storage
        .workflows
        .list_workflows()
        .context("Failed to list workflows")
}

pub async fn get_workflow(core: &Arc<AppCore>, id: &str) -> Result<Workflow> {
    core.storage
        .workflows
        .get_workflow(id)
        .with_context(|| format!("Failed to get workflow {}", id))
}

pub async fn create_workflow(core: &Arc<AppCore>, mut workflow: Workflow) -> Result<Workflow> {
    // Generate ID if not provided
    if workflow.id.is_empty() {
        workflow.id = format!("wf_{}", uuid::Uuid::new_v4());
    }

    core.storage
        .workflows
        .create_workflow(&workflow)
        .with_context(|| format!("Failed to save workflow {}", workflow.name))?;

    Ok(workflow)
}

pub async fn update_workflow(
    core: &Arc<AppCore>,
    id: &str,
    workflow: Workflow,
) -> Result<Workflow> {
    core.storage
        .workflows
        .update_workflow(id, &workflow)
        .with_context(|| format!("Failed to update workflow {}", id))?;

    Ok(workflow)
}

pub async fn delete_workflow(core: &Arc<AppCore>, id: &str) -> Result<()> {
    // Try to deactivate any active triggers for this workflow
    let _ = core.trigger_manager.deactivate_workflow(id).await;

    core.storage
        .workflows
        .delete_workflow(id)
        .with_context(|| format!("Failed to delete workflow {}", id))
}

// Execution functions (also in workflows API)

pub async fn execute_workflow_inline(core: &Arc<AppCore>, mut workflow: Workflow) -> Result<Value> {
    workflow.id = format!("inline-{}", uuid::Uuid::new_v4());

    let mut wf_executor =
        WorkflowExecutor::new_sync(workflow, Some(core.storage.clone()), core.registry.clone());
    wf_executor
        .execute()
        .await
        .context("Workflow execution failed")
}

pub async fn execute_workflow_by_id(
    core: &Arc<AppCore>,
    workflow_id: &str,
    input: Value,
) -> Result<Value> {
    let workflow = core
        .storage
        .workflows
        .get_workflow(workflow_id)
        .with_context(|| format!("Failed to get workflow {} for execution", workflow_id))?;

    let mut wf_executor =
        WorkflowExecutor::new_sync(workflow, Some(core.storage.clone()), core.registry.clone());
    wf_executor.set_input(input);
    wf_executor
        .execute()
        .await
        .with_context(|| format!("Failed to execute workflow {}", workflow_id))
}

pub async fn submit_workflow(
    core: &Arc<AppCore>,
    workflow_id: &str,
    input: Value,
) -> Result<String> {
    core.executor
        .submit(workflow_id.to_string(), input)
        .await
        .with_context(|| format!("Failed to submit workflow {}", workflow_id))
}

pub async fn get_execution_status(
    core: &Arc<AppCore>,
    execution_id: &str,
) -> Result<Vec<crate::models::Task>> {
    core.executor
        .get_execution_status(execution_id)
        .await
        .with_context(|| format!("Failed to get execution status for {}", execution_id))
}
