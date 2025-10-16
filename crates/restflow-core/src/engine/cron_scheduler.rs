use crate::engine::executor::WorkflowExecutor;
use crate::models::ActiveTrigger;
use crate::storage::Storage;
use anyhow::{Result, anyhow};
use chrono_tz::Tz;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Cron scheduler responsible for managing all scheduled jobs
///
/// Features:
/// - Supports standard cron expressions
/// - Supports timezone configuration
/// - Automatically triggers workflow execution
/// - Persists job mapping relationships
pub struct CronScheduler {
    /// tokio-cron-scheduler instance
    scheduler: JobScheduler,
    /// Storage layer used to fetch workflows
    storage: Arc<Storage>,
    /// Workflow executor
    executor: Arc<WorkflowExecutor>,
    /// job_uuid -> trigger_id mapping table (used for job removal)
    job_map: Arc<RwLock<HashMap<Uuid, String>>>,
}

impl CronScheduler {
    /// Create a new Cron scheduler
    pub async fn new(storage: Arc<Storage>, executor: Arc<WorkflowExecutor>) -> Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| anyhow!("Failed to create JobScheduler: {}", e))?;

        Ok(Self {
            scheduler,
            storage,
            executor,
            job_map: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<()> {
        self.scheduler
            .start()
            .await
            .map_err(|e| anyhow!("Failed to start scheduler: {}", e))?;

        info!("CronScheduler started successfully");
        Ok(())
    }

    /// Add a scheduled job
    ///
    /// # Parameters
    /// - `trigger`: active trigger information
    /// - `cron_expr`: 6-field cron expression (sec min hour day month weekday), e.g. "0 0 0 * * *" for midnight every day
    /// - `timezone`: timezone (e.g. "Asia/Shanghai"), None indicates UTC
    /// - `payload`: payload passed to the workflow when triggered
    pub async fn add_schedule(
        &self,
        trigger: &ActiveTrigger,
        cron_expr: String,
        timezone: Option<String>,
        payload: Option<Value>,
    ) -> Result<()> {
        let trigger_id = trigger.id.clone();
        let workflow_id = trigger.workflow_id.clone();
        let executor = self.executor.clone();
        let storage = self.storage.clone();

        // Prepare payload (use an empty object when not provided)
        let trigger_payload = payload.unwrap_or(Value::Object(Default::default()));

        debug!(
            trigger_id = %trigger_id,
            workflow_id = %workflow_id,
            cron = %cron_expr,
            timezone = ?timezone,
            "Adding cron schedule"
        );

        // Create the job
        let job = if let Some(tz) = timezone {
            // Job with timezone support
            let timezone: Tz = Tz::from_str(&tz).map_err(|e| anyhow!("Invalid timezone {}: {}", tz, e))?;
            Job::new_async_tz(cron_expr.as_str(), timezone, move |_uuid, _l| {
                let workflow_id = workflow_id.clone();
                let executor = executor.clone();
                let storage = storage.clone();
                let payload = trigger_payload.clone();
                let trigger_id = trigger_id.clone();

                Box::pin(async move {
                    info!(
                        trigger_id = %trigger_id,
                        workflow_id = %workflow_id,
                        "Cron job triggered"
                    );

                    // Submit workflow execution
                    match executor.submit(workflow_id.clone(), payload).await {
                        Ok(execution_id) => {
                            info!(
                                execution_id = %execution_id,
                                workflow_id = %workflow_id,
                                "Workflow execution submitted by cron trigger"
                            );

                            // Update trigger statistics
                            if let Err(e) = update_trigger_stats(&storage, &trigger_id) {
                                error!(error = ?e, "Failed to update trigger statistics");
                            }
                        }
                        Err(e) => {
                            error!(
                                error = ?e,
                                workflow_id = %workflow_id,
                                "Failed to submit workflow execution"
                            );
                        }
                    }
                })
            })
            .map_err(|e| anyhow!("Failed to create cron job with timezone: {}", e))?
        } else {
            // Job in UTC
            Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
                let workflow_id = workflow_id.clone();
                let executor = executor.clone();
                let storage = storage.clone();
                let payload = trigger_payload.clone();
                let trigger_id = trigger_id.clone();

                Box::pin(async move {
                    info!(
                        trigger_id = %trigger_id,
                        workflow_id = %workflow_id,
                        "Cron job triggered (UTC)"
                    );

                    // Submit workflow execution
                    match executor.submit(workflow_id.clone(), payload).await {
                        Ok(execution_id) => {
                            info!(
                                execution_id = %execution_id,
                                workflow_id = %workflow_id,
                                "Workflow execution submitted by cron trigger"
                            );

                            // Update trigger statistics
                            if let Err(e) = update_trigger_stats(&storage, &trigger_id) {
                                error!(error = ?e, "Failed to update trigger statistics");
                            }
                        }
                        Err(e) => {
                            error!(
                                error = ?e,
                                workflow_id = %workflow_id,
                                "Failed to submit workflow execution"
                            );
                        }
                    }
                })
            })
            .map_err(|e| anyhow!("Failed to create cron job: {}", e))?
        };

        // Add job to the scheduler
        let job_uuid = self.scheduler.add(job).await.map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        // Record the mapping
        self.job_map.write().await.insert(job_uuid, trigger.id.clone());

        info!(
            trigger_id = %trigger.id,
            job_uuid = %job_uuid,
            cron = %cron_expr,
            "Cron schedule added successfully"
        );

        Ok(())
    }

    /// Remove a scheduled job
    ///
    /// Returns Ok(true) when a job was found and removed, Ok(false) when no job existed,
    /// and Err if the underlying scheduler operation failed.
    pub async fn remove_schedule(&self, trigger_id: &str) -> Result<bool> {
        // Find the corresponding job_uuid
        let job_uuid = {
            let map = self.job_map.read().await;
            map.iter()
                .find_map(|(uuid, id)| if id == trigger_id { Some(*uuid) } else { None })
        };

        if let Some(uuid) = job_uuid {
            // Remove job from the scheduler
            self.scheduler
                .remove(&uuid)
                .await
                .map_err(|e| anyhow!("Failed to remove job from scheduler: {}", e))?;

            // Remove from the mapping table
            self.job_map.write().await.remove(&uuid);

            info!(trigger_id = %trigger_id, job_uuid = %uuid, "Cron schedule removed");
            Ok(true)
        } else {
            debug!(
                trigger_id = %trigger_id,
                "No cron job found for trigger (might already be removed)"
            );
            Ok(false)
        }
    }

    /// Shut down the scheduler
    pub async fn shutdown(&mut self) -> Result<()> {
        self.scheduler
            .shutdown()
            .await
            .map_err(|e| anyhow!("Failed to shutdown scheduler: {}", e))?;

        info!("CronScheduler shutdown successfully");
        Ok(())
    }

    /// Get the number of active jobs
    pub async fn active_job_count(&self) -> usize {
        self.job_map.read().await.len()
    }
}

/// Update trigger statistics
fn update_trigger_stats(storage: &Storage, trigger_id: &str) -> Result<()> {
    if let Some(mut trigger) = storage.triggers.get_active_trigger(trigger_id)? {
        trigger.record_trigger();
        storage.triggers.update_trigger(&trigger)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TriggerConfig, Workflow, Node, NodeType};
    use crate::node::registry::NodeRegistry;
    use tempfile::tempdir;

    async fn setup_test_scheduler() -> (CronScheduler, Arc<Storage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()).unwrap());
        let registry = Arc::new(NodeRegistry::new());

        // Create a simple test workflow
        let workflow = Workflow {
            id: "test-workflow".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![Node {
                id: "test-node".to_string(),
                node_type: NodeType::Print,
                config: serde_json::json!({"message": "Test"}),
                position: None,
            }],
            edges: vec![],
        };
        storage.workflows.create_workflow(&workflow).unwrap();

        let executor = Arc::new(WorkflowExecutor::new_async(
            storage.clone(),
            4,
            registry,
        ));

        let scheduler = CronScheduler::new(storage.clone(), executor)
            .await
            .unwrap();

        (scheduler, storage, temp_dir)
    }

    #[tokio::test]
    async fn test_add_and_remove_schedule() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(), // 6-field format: sec min hour day month weekday
                timezone: None,
                payload: None,
            },
        );

        // Add schedule
        scheduler
            .add_schedule(&trigger, "0 0 * * * *".to_string(), None, None)
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 1);

        // Remove schedule
        let removed = scheduler.remove_schedule(&trigger.id).await.unwrap();
        assert!(removed);

        assert_eq!(scheduler.active_job_count().await, 0);

        scheduler.shutdown().await.unwrap();
    }
}
