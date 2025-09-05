use crate::{AppCore, engine::trigger_manager::{WebhookResponse, TriggerStatus}, models::ActiveTrigger};
use serde_json::Value;
use std::sync::Arc;
use std::collections::HashMap;

// Trigger management functions

pub async fn activate_workflow(core: &Arc<AppCore>, workflow_id: &str) -> Result<(), String> {
    core.trigger_manager.activate_workflow(workflow_id).await
        .map_err(|e| format!("Failed to activate workflow: {}", e))?;
    Ok(())
}

pub async fn deactivate_workflow(core: &Arc<AppCore>, workflow_id: &str) -> Result<(), String> {
    core.trigger_manager.deactivate_workflow(workflow_id).await
        .map_err(|e| format!("Failed to deactivate workflow: {}", e))
}

pub async fn get_workflow_trigger_status(
    core: &Arc<AppCore>, 
    workflow_id: &str
) -> Result<Option<TriggerStatus>, String> {
    core.trigger_manager.get_trigger_status(workflow_id).await
        .map_err(|e| format!("Failed to get trigger status: {}", e))
}

pub async fn test_workflow_trigger(
    core: &Arc<AppCore>,
    workflow_id: &str,
    test_input: Value
) -> Result<String, String> {
    // Use async submission for consistency with API layer
    core.executor.submit(workflow_id.to_string(), test_input).await
        .map_err(|e| format!("Test execution failed: {}", e))
}

pub async fn handle_webhook_trigger(
    core: &Arc<AppCore>,
    webhook_id: &str,
    method: &str,
    headers: HashMap<String, String>,
    body: Value
) -> Result<WebhookResponse, String> {
    // Use the trigger manager to handle webhook properly
    core.trigger_manager.handle_webhook(webhook_id, method, headers, body).await
        .map_err(|e| format!("Webhook handling failed: {}", e))
}

pub async fn list_active_triggers(core: &Arc<AppCore>) -> Result<Vec<ActiveTrigger>, String> {
    core.storage.triggers.list_active_triggers()
        .map_err(|e| format!("Failed to list active triggers: {}", e))
}