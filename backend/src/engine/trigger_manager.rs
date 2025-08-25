use crate::models::{ActiveTrigger, TriggerConfig, AuthConfig, ResponseMode};
use crate::storage::Storage;
use crate::engine::executor::AsyncWorkflowExecutor;
use std::sync::Arc;
use std::collections::HashMap;
use serde_json::Value;
use anyhow::{Result, anyhow};

pub struct TriggerManager {
    storage: Arc<Storage>,
    executor: Arc<AsyncWorkflowExecutor>,
}

impl TriggerManager {
    pub fn new(storage: Arc<Storage>, executor: Arc<AsyncWorkflowExecutor>) -> Self {
        Self {
            storage,
            executor,
        }
    }
    
    // Initialize trigger manager
    pub async fn init(&self) -> Result<()> {
        let triggers = self.storage.triggers.list_active_triggers()?;
        let webhook_count = triggers.iter()
            .filter(|t| matches!(t.trigger_config, TriggerConfig::Webhook { .. }))
            .count();
        
        println!("TriggerManager initialized with {} active triggers ({} webhooks)", 
            triggers.len(), webhook_count);
        Ok(())
    }
    
    // Activate workflow trigger
    pub async fn activate_workflow(&self, workflow_id: &str) -> Result<ActiveTrigger> {

        if let Some(existing) = self.storage.triggers.get_active_trigger_by_workflow(workflow_id)? {
            return Err(anyhow!("Workflow {} already has an active trigger", workflow_id));
        }
        
        let workflow = self.storage.workflows.get_workflow(workflow_id)
            .map_err(|e| anyhow!("Failed to get workflow: {}", e))?;
        
        let trigger_config = workflow.extract_trigger_config()
            .ok_or_else(|| anyhow!("Workflow {} has no trigger configuration", workflow_id))?;
        
        let active_trigger = ActiveTrigger::new(workflow_id.to_string(), trigger_config.clone());
        
        self.storage.triggers.activate_trigger(&active_trigger)?;
        
        println!("Activated trigger for workflow {}: {:?}", workflow_id, trigger_config);
        
        Ok(active_trigger)
    }
    
    // Deactivate workflow trigger
    pub async fn deactivate_workflow(&self, workflow_id: &str) -> Result<()> {

        let trigger = self.storage.triggers.get_active_trigger_by_workflow(workflow_id)?
            .ok_or_else(|| anyhow!("No active trigger found for workflow {}", workflow_id))?;
        
        self.storage.triggers.deactivate_trigger(&trigger.id)?;
        
        println!("Deactivated trigger for workflow {}", workflow_id);
        
        Ok(())
    }
    
    // Handle webhook trigger
    pub async fn handle_webhook(
        &self,
        webhook_id: &str,
        method: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<WebhookResponse> {
        // Find workflow_id from storage
        let workflow_id = self.storage.triggers.get_workflow_by_webhook(webhook_id)?
            .ok_or_else(|| anyhow!("Webhook {} not found", webhook_id))?;
        
        // Get trigger config
        let mut trigger = self.storage.triggers.get_active_trigger(webhook_id)?
            .ok_or_else(|| anyhow!("Trigger {} not found", webhook_id))?;
        
        // Verify HTTP method and process webhook
        if let TriggerConfig::Webhook { method: expected_method, auth, response_mode, .. } = &trigger.trigger_config {
            if expected_method.to_uppercase() != method.to_uppercase() {
                return Err(anyhow!("Method not allowed. Expected {}", expected_method));
            }
            
            // Verify authentication
            if let Some(auth_config) = auth {
                self.verify_auth(auth_config, &headers)?;
            }
            
            // Prepare input data
            let input = serde_json::json!({
                "headers": headers,
                "body": body,
                "method": method,
                "webhook_id": webhook_id,
                "triggered_at": chrono::Utc::now().to_rfc3339(),
            });
            
            // Handle based on response mode
            let response = match response_mode {
                ResponseMode::Async => {
                    // Async mode: return execution_id immediately
                    let execution_id = self.executor.submit(workflow_id.clone(), input).await
                        .map_err(|e| anyhow!("Failed to submit workflow: {}", e))?;
                    
                    WebhookResponse::Async { execution_id }
                }
                ResponseMode::Sync => {
                    // Sync mode: execute directly without queue
                    use crate::engine::executor::WorkflowExecutor;
                    
                    // Load workflow
                    let workflow = self.storage.workflows.get_workflow(&workflow_id)
                        .map_err(|e| anyhow!("Failed to load workflow: {}", e))?;
                    
                    // Create executor and execute synchronously
                    let mut executor = WorkflowExecutor::new(workflow);
                    executor.set_input(input);
                    
                    let result = executor.execute().await
                        .map_err(|e| anyhow!("Workflow execution failed: {}", e))?;
                    
                    WebhookResponse::Sync { result }
                }
            };
            
            // Update trigger statistics
            trigger.record_trigger();
            self.storage.triggers.update_trigger(&trigger)?;
            
            Ok(response)
        } else {
            Err(anyhow!("Trigger {} is not a webhook trigger", webhook_id))
        }
    }
    
    // Verify authentication
    fn verify_auth(&self, auth_config: &AuthConfig, headers: &HashMap<String, String>) -> Result<()> {
        match auth_config {
            AuthConfig::None => Ok(()),
            AuthConfig::ApiKey { key, header_name } => {
                let header = header_name.as_deref().unwrap_or("X-API-Key");
                let provided_key = headers.get(header)
                    .ok_or_else(|| anyhow!("Missing API key header: {}", header))?;
                
                if provided_key != key {
                    Err(anyhow!("Invalid API key"))
                } else {
                    Ok(())
                }
            }
            AuthConfig::Basic { username, password } => {
                let auth_header = headers.get("authorization")
                    .ok_or_else(|| anyhow!("Missing Authorization header"))?;
                
                if !auth_header.starts_with("Basic ") {
                    return Err(anyhow!("Invalid Authorization header format"));
                }
                
                let encoded = &auth_header[6..];
                let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                    .map_err(|_| anyhow!("Invalid base64 encoding"))?;
                let credentials = String::from_utf8(decoded)
                    .map_err(|_| anyhow!("Invalid UTF-8 in credentials"))?;
                
                let parts: Vec<&str> = credentials.splitn(2, ':').collect();
                if parts.len() != 2 {
                    return Err(anyhow!("Invalid credentials format"));
                }
                
                if parts[0] != username || parts[1] != password {
                    Err(anyhow!("Invalid username or password"))
                } else {
                    Ok(())
                }
            }
        }
    }
    
    // Get workflow trigger status
    pub async fn get_trigger_status(&self, workflow_id: &str) -> Result<Option<TriggerStatus>> {
        let workflow = self.storage.workflows.get_workflow(workflow_id)
            .map_err(|e| anyhow!("Failed to get workflow: {}", e))?;
        
        if let Some(trigger) = self.storage.triggers.get_active_trigger_by_workflow(workflow_id)? {
            let webhook_url = if matches!(trigger.trigger_config, TriggerConfig::Webhook { .. }) {
                Some(format!("/api/triggers/webhook/{}", trigger.id))
            } else {
                None
            };
            
            Ok(Some(TriggerStatus {
                is_active: true,
                trigger_config: trigger.trigger_config.clone(),
                webhook_url,
                trigger_count: trigger.trigger_count,
                last_triggered_at: trigger.last_triggered_at,
                activated_at: trigger.activated_at,
            }))
        } else if let Some(config) = workflow.trigger_config {
            Ok(Some(TriggerStatus {
                is_active: false,
                trigger_config: config,
                webhook_url: None,
                trigger_count: 0,
                last_triggered_at: None,
                activated_at: 0,
            }))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
pub enum WebhookResponse {
    Async { execution_id: String },
    Sync { result: Value },
}

#[derive(Debug, serde::Serialize)]
pub struct TriggerStatus {
    pub is_active: bool,
    pub trigger_config: TriggerConfig,
    pub webhook_url: Option<String>,
    pub trigger_count: u64,
    pub last_triggered_at: Option<i64>,
    pub activated_at: i64,
}