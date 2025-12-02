use crate::{
    AppCore,
    engine::trigger_manager::{TriggerStatus, WebhookResponse},
    models::ActiveTrigger,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

// Trigger management functions

pub fn generate_test_execution_id() -> String {
    format!("test-{}", Uuid::new_v4())
}

pub async fn activate_workflow(core: &Arc<AppCore>, workflow_id: &str) -> Result<()> {
    core.trigger_manager
        .activate_workflow(workflow_id)
        .await
        .with_context(|| format!("Failed to activate workflow {}", workflow_id))?;
    Ok(())
}

pub async fn deactivate_workflow(core: &Arc<AppCore>, workflow_id: &str) -> Result<()> {
    core.trigger_manager
        .deactivate_workflow(workflow_id)
        .await
        .with_context(|| format!("Failed to deactivate workflow {}", workflow_id))
}

pub async fn get_workflow_trigger_status(
    core: &Arc<AppCore>,
    workflow_id: &str,
) -> Result<Option<TriggerStatus>> {
    core.trigger_manager
        .get_trigger_status(workflow_id)
        .await
        .with_context(|| format!("Failed to get trigger status for workflow {}", workflow_id))
}

pub async fn test_workflow_trigger(
    core: &Arc<AppCore>,
    workflow_id: &str,
    test_input: Value,
) -> Result<String> {
    let test_execution_id = generate_test_execution_id();
    core.executor
        .submit_with_execution_id(workflow_id.to_string(), test_input, test_execution_id)
        .await
        .with_context(|| format!("Test execution failed for workflow {}", workflow_id))
}

pub async fn handle_webhook_trigger(
    core: &Arc<AppCore>,
    webhook_id: &str,
    method: &str,
    headers: HashMap<String, String>,
    body: Value,
) -> Result<WebhookResponse> {
    // Use the trigger manager to handle webhook properly
    core.trigger_manager
        .handle_webhook(webhook_id, method, headers, body)
        .await
        .with_context(|| format!("Webhook handling failed for {}", webhook_id))
}

pub async fn list_active_triggers(core: &Arc<AppCore>) -> Result<Vec<ActiveTrigger>> {
    core.storage
        .triggers
        .list_active_triggers()
        .context("Failed to list active triggers")
}
