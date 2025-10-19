use crate::engine::cron_scheduler::CronScheduler;
use crate::engine::executor::WorkflowExecutor;
use crate::models::{ActiveTrigger, AuthConfig, TriggerConfig};
use crate::storage::Storage;
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use ts_rs::TS;

pub struct TriggerManager {
    storage: Arc<Storage>,
    executor: Arc<WorkflowExecutor>,
    cron_scheduler: Arc<CronScheduler>,
}

impl TriggerManager {
    pub fn new(
        storage: Arc<Storage>,
        executor: Arc<WorkflowExecutor>,
        cron_scheduler: Arc<CronScheduler>,
    ) -> Self {
        Self {
            storage,
            executor,
            cron_scheduler,
        }
    }

    pub async fn init(&self) -> Result<()> {
        let triggers = self.storage.triggers.list_active_triggers()?;
        let webhook_count = triggers
            .iter()
            .filter(|t| matches!(t.trigger_config, TriggerConfig::Webhook { .. }))
            .count();
        let schedule_count = triggers
            .iter()
            .filter(|t| matches!(t.trigger_config, TriggerConfig::Schedule { .. }))
            .count();

        info!(
            active_triggers = triggers.len(),
            webhooks = webhook_count,
            schedules = schedule_count,
            "TriggerManager initialized"
        );

        for trigger in triggers.iter() {
            if let TriggerConfig::Schedule {
                cron,
                timezone,
                payload,
            } = &trigger.trigger_config
            {
                info!(
                    trigger_id = %trigger.id,
                    workflow_id = %trigger.workflow_id,
                    cron = %cron,
                    "Restoring schedule trigger"
                );

                if let Err(e) = self
                    .cron_scheduler
                    .add_schedule(trigger, cron.clone(), timezone.clone(), payload.clone())
                    .await
                {
                    tracing::error!(
                        error = ?e,
                        trigger_id = %trigger.id,
                        "Failed to restore schedule trigger"
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn activate_workflow(&self, workflow_id: &str) -> Result<Vec<ActiveTrigger>> {
        let workflow = self
            .storage
            .workflows
            .get_workflow(workflow_id)
            .map_err(|e| anyhow!("Failed to get workflow: {}", e))?;

        let trigger_configs = workflow.extract_trigger_configs();

        if trigger_configs.is_empty() {
            return Err(anyhow!("Workflow {} has no trigger nodes", workflow_id));
        }

        let mut activated_triggers = Vec::new();

        for (node_id, trigger_config) in trigger_configs {
            let trigger_id = format!("{}_{}", workflow_id, node_id);
            let existing = self.storage.triggers.get_active_trigger(&trigger_id)?;
            if existing.is_some() {
                debug!(trigger_id = %trigger_id, "Trigger already active");
                continue;
            }

            let mut active_trigger =
                ActiveTrigger::new(workflow_id.to_string(), trigger_config.clone());
            active_trigger.id = trigger_id;

            self.storage.triggers.activate_trigger(&active_trigger)?;

            if let TriggerConfig::Schedule {
                cron,
                timezone,
                payload,
            } = &trigger_config
            {
                self.cron_scheduler
                    .add_schedule(
                        &active_trigger,
                        cron.clone(),
                        timezone.clone(),
                        payload.clone(),
                    )
                    .await?;
            }

            info!(node_id = %node_id, workflow_id = %workflow_id, config = ?trigger_config, "Trigger activated");
            activated_triggers.push(active_trigger);
        }

        Ok(activated_triggers)
    }

    pub async fn deactivate_workflow(&self, workflow_id: &str) -> Result<()> {
        let trigger = self
            .storage
            .triggers
            .get_active_trigger_by_workflow(workflow_id)?
            .ok_or_else(|| anyhow!("No active trigger found for workflow {}", workflow_id))?;

        if matches!(trigger.trigger_config, TriggerConfig::Schedule { .. }) {
            match self.cron_scheduler.remove_schedule(&trigger.id).await {
                Ok(true) => {
                    tracing::debug!(
                        trigger_id = %trigger.id,
                        "Schedule removed from cron scheduler"
                    );
                }
                Ok(false) => {
                    tracing::debug!(
                        trigger_id = %trigger.id,
                        "Schedule not found in cron scheduler (likely already removed)"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        trigger_id = %trigger.id,
                        "Failed to remove schedule from cron scheduler"
                    );
                    return Err(e);
                }
            }
        }

        self.storage.triggers.deactivate_trigger(&trigger.id)?;

        info!(workflow_id = %workflow_id, "Trigger deactivated");

        Ok(())
    }

    pub async fn handle_webhook(
        &self,
        webhook_id: &str,
        method: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<WebhookResponse> {
        let workflow_id = self
            .storage
            .triggers
            .get_workflow_by_webhook(webhook_id)?
            .ok_or_else(|| anyhow!("Webhook {} not found", webhook_id))?;

        let mut trigger = self
            .storage
            .triggers
            .get_active_trigger(webhook_id)?
            .ok_or_else(|| anyhow!("Trigger {} not found", webhook_id))?;

        if let TriggerConfig::Webhook {
            method: expected_method,
            auth,
            ..
        } = &trigger.trigger_config
        {
            if expected_method.to_uppercase() != method.to_uppercase() {
                return Err(anyhow!("Method not allowed. Expected {}", expected_method));
            }

            if let Some(auth_config) = auth {
                self.verify_auth(auth_config, &headers)?;
            }

            let input = serde_json::json!({
                "headers": headers,
                "body": body,
                "method": method,
                "webhook_id": webhook_id,
                "triggered_at": chrono::Utc::now().to_rfc3339(),
            });

            let execution_id = self
                .executor
                .submit(workflow_id.clone(), input)
                .await
                .map_err(|e| anyhow!("Failed to submit workflow: {}", e))?;

            let response = WebhookResponse::Async { execution_id };

            trigger.record_trigger();
            self.storage.triggers.update_trigger(&trigger)?;

            Ok(response)
        } else {
            Err(anyhow!("Trigger {} is not a webhook trigger", webhook_id))
        }
    }

    fn verify_auth(
        &self,
        auth_config: &AuthConfig,
        headers: &HashMap<String, String>,
    ) -> Result<()> {
        match auth_config {
            AuthConfig::None => Ok(()),
            AuthConfig::ApiKey { key, header_name } => {
                let header = header_name.as_deref().unwrap_or("X-API-Key");
                let provided_key = headers
                    .get(header)
                    .ok_or_else(|| anyhow!("Missing API key header: {}", header))?;

                if provided_key != key {
                    Err(anyhow!("Invalid API key"))
                } else {
                    Ok(())
                }
            }
            AuthConfig::Basic { username, password } => {
                let auth_header = headers
                    .get("authorization")
                    .ok_or_else(|| anyhow!("Missing Authorization header"))?;

                if !auth_header.starts_with("Basic ") {
                    return Err(anyhow!("Invalid Authorization header format"));
                }

                let encoded = &auth_header[6..];
                let decoded =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
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

    pub async fn get_trigger_status(&self, workflow_id: &str) -> Result<Option<TriggerStatus>> {
        let workflow = self
            .storage
            .workflows
            .get_workflow(workflow_id)
            .map_err(|e| anyhow!("Failed to get workflow: {}", e))?;

        if let Some(trigger) = self
            .storage
            .triggers
            .get_active_trigger_by_workflow(workflow_id)?
        {
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
        } else {
            let configs = workflow.extract_trigger_configs();
            if let Some((_, config)) = configs.first() {
                Ok(Some(TriggerStatus {
                    is_active: false,
                    trigger_config: config.clone(),
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
}

#[derive(Debug)]
pub enum WebhookResponse {
    Async { execution_id: String },
    // Sync mode removed - Webhooks now use async mode only
}

#[derive(Debug, serde::Serialize, TS)]
#[ts(export)]
pub struct TriggerStatus {
    pub is_active: bool,
    pub trigger_config: TriggerConfig,
    pub webhook_url: Option<String>,
    pub trigger_count: u64,
    pub last_triggered_at: Option<i64>,
    pub activated_at: i64,
}
