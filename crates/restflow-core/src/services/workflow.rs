use crate::{
    AppCore,
    engine::context::namespace,
    models::{NodeType, TaskStatus, Workflow},
};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::{Duration, Instant, sleep};
use tracing::{error, warn};

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

    if workflow.nodes.iter().any(|node| node.node_type == NodeType::Python)
        && let Err(e) = core
            .get_python_manager()
            .await
            .context("Failed to initialize Python manager for inline execution")
    {
        error!(
            workflow_id = %workflow.id,
            workflow_name = %workflow.name,
            error = %e,
            "Failed to initialize Python manager for inline execution"
        );
        return Err(e);
    }

    if let Err(e) = core.storage.workflows.create_workflow(&workflow) {
        error!(
            workflow_id = %workflow.id,
            workflow_name = %workflow.name,
            error = %e,
            "Failed to persist inline workflow"
        );
        return Err(e).with_context(|| format!("Failed to persist inline workflow {}", workflow.name));
    }

    let result = async {
        let execution_id = match core.executor.submit(workflow.id.clone(), Value::Null).await {
            Ok(id) => id,
            Err(e) => {
                error!(
                    workflow_id = %workflow.id,
                    workflow_name = %workflow.name,
                    error = %e,
                    "Failed to submit workflow to executor"
                );
                return Err(e).with_context(|| format!("Failed to submit workflow {}", workflow.id));
            }
        };

        wait_and_collect(core, &execution_id).await
    }
    .await;

    if let Err(e) = core.storage.workflows.delete_workflow(&workflow.id) {
        warn!(
            workflow_id = %workflow.id,
            error = %e,
            "Failed to clean up inline workflow after execution"
        );
    }

    result
}

pub async fn execute_workflow_by_id(
    core: &Arc<AppCore>,
    workflow_id: &str,
    input: Value,
) -> Result<Value> {
    // Load workflow to check if Python manager initialization is needed
    let workflow = core.storage
        .workflows
        .get_workflow(workflow_id)
        .with_context(|| format!("Failed to load workflow {}", workflow_id))?;

    // Ensure Python manager is initialized if workflow contains Python nodes
    if workflow.nodes.iter().any(|node| node.node_type == NodeType::Python) {
        core.get_python_manager()
            .await
            .context("Failed to initialize Python manager for workflow execution")?;
    }

    let execution_id = core
        .executor
        .submit(workflow_id.to_string(), input)
        .await
        .with_context(|| format!("Failed to execute workflow {}", workflow_id))?;

    wait_and_collect(core, &execution_id).await
}

pub async fn submit_workflow(
    core: &Arc<AppCore>,
    workflow_id: &str,
    input: Value,
) -> Result<String> {
    // Load workflow to check if Python manager initialization is needed
    let workflow = core.storage
        .workflows
        .get_workflow(workflow_id)
        .with_context(|| format!("Failed to load workflow {}", workflow_id))?;

    // Ensure Python manager is initialized if workflow contains Python nodes
    if workflow.nodes.iter().any(|node| node.node_type == NodeType::Python) {
        core.get_python_manager()
            .await
            .context("Failed to initialize Python manager for workflow submission")?;
    }

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

const EXECUTION_POLL_INTERVAL: Duration = Duration::from_millis(100);
const EXECUTION_TIMEOUT: Duration = Duration::from_secs(60);

async fn wait_and_collect(core: &Arc<AppCore>, execution_id: &str) -> Result<Value> {
    let tasks = wait_for_completion(core, execution_id).await?;

    if let Some(failed_task) = tasks.iter().find(|task| task.status == TaskStatus::Failed) {
        let error_message = failed_task
            .error
            .clone()
            .unwrap_or_else(|| "Workflow execution failed".to_string());
        bail!(error_message);
    }

    Ok(build_execution_context(execution_id, &tasks))
}

async fn wait_for_completion(
    core: &Arc<AppCore>,
    execution_id: &str,
) -> Result<Vec<crate::models::Task>> {
    let deadline = Instant::now() + EXECUTION_TIMEOUT;

    loop {
        let tasks = core.executor.get_execution_status(execution_id).await?;

        if !tasks.is_empty()
            && tasks
                .iter()
                .all(|task| matches!(task.status, TaskStatus::Completed | TaskStatus::Failed))
        {
            return Ok(tasks);
        }

        if Instant::now() >= deadline {
            bail!("Execution {} timed out", execution_id);
        }

        sleep(EXECUTION_POLL_INTERVAL).await;
    }
}

fn build_execution_context(execution_id: &str, tasks: &[crate::models::Task]) -> Value {
    let workflow_id = tasks
        .first()
        .map(|task| task.workflow_id.clone())
        .unwrap_or_default();

    let mut data = serde_json::Map::new();
    let mut seen_keys: HashSet<String> = HashSet::new();

    for task in tasks {
        for (key, value) in &task.context.data {
            if seen_keys.insert(key.clone()) {
                data.insert(key.clone(), value.clone());
            }
        }

        if let Some(output) = &task.output {
            let key = namespace::node(&task.node_id);
            // Serialize NodeOutput to Value for context storage
            if let Ok(output_value) = serde_json::to_value(output) {
                data.insert(key, output_value);
            }
        }
    }

    serde_json::json!({
        "workflow_id": workflow_id,
        "execution_id": execution_id,
        "data": data,
    })
}
