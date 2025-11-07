use crate::engine::executor::WorkflowExecutor;
use crate::models::ActiveTrigger;
use crate::storage::Storage;
use anyhow::{Result, anyhow};
use chrono_tz::Tz;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
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
    /// Tracks whether shutdown has been requested to prevent new schedules
    is_shutdown: AtomicBool,
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
            is_shutdown: AtomicBool::new(false),
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
        if self.is_shutdown.load(Ordering::SeqCst) {
            return Err(anyhow!("Scheduler has been shutdown"));
        }

        let trigger_id = trigger.id.clone();
        let workflow_id = trigger.workflow_id.clone();
        let executor = self.executor.clone();
        let storage = self.storage.clone();

        let trigger_payload = payload.unwrap_or(Value::Object(Default::default()));

        debug!(
            trigger_id = %trigger_id,
            workflow_id = %workflow_id,
            cron = %cron_expr,
            timezone = ?timezone,
            "Adding cron schedule"
        );

        let job = if let Some(tz) = timezone {
            let timezone: Tz =
                Tz::from_str(&tz).map_err(|e| anyhow!("Invalid timezone {}: {}", tz, e))?;
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

                    match executor.submit(workflow_id.clone(), payload).await {
                        Ok(execution_id) => {
                            info!(
                                execution_id = %execution_id,
                                workflow_id = %workflow_id,
                                "Workflow execution submitted by cron trigger"
                            );

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

                    match executor.submit(workflow_id.clone(), payload).await {
                        Ok(execution_id) => {
                            info!(
                                execution_id = %execution_id,
                                workflow_id = %workflow_id,
                                "Workflow execution submitted by cron trigger"
                            );

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

        let job_uuid = self
            .scheduler
            .add(job)
            .await
            .map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        self.job_map
            .write()
            .await
            .insert(job_uuid, trigger.id.clone());

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
        let job_uuid = {
            let map = self.job_map.read().await;
            map.iter()
                .find_map(|(uuid, id)| if id == trigger_id { Some(*uuid) } else { None })
        };

        if let Some(uuid) = job_uuid {
            self.scheduler
                .remove(&uuid)
                .await
                .map_err(|e| anyhow!("Failed to remove job from scheduler: {}", e))?;

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
        self.is_shutdown.store(true, Ordering::SeqCst);

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
    use crate::models::{Node, NodeType, TriggerConfig, Workflow};
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
                config: serde_json::json!({
                    "type": "Print",
                    "data": {
                        "message": "Test message"
                    }
                }),
                position: None,
            }],
            edges: vec![],
        };
        storage.workflows.create_workflow(&workflow).unwrap();

        let executor = Arc::new(WorkflowExecutor::new(storage.clone(), 4, registry));

        let scheduler = CronScheduler::new(storage.clone(), executor).await.unwrap();

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

    #[tokio::test]
    async fn test_start_and_shutdown() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        // Start scheduler
        scheduler.start().await.unwrap();

        // Add a test job to verify scheduler is running
        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(),
                timezone: None,
                payload: None,
            },
        );

        scheduler
            .add_schedule(&trigger, "0 0 * * * *".to_string(), None, None)
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 1);

        // Shutdown scheduler
        scheduler.shutdown().await.unwrap();

        // Verify job count is still maintained (job_map is not cleared)
        assert_eq!(scheduler.active_job_count().await, 1);
    }

    #[tokio::test]
    async fn test_invalid_cron_expression() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "invalid cron".to_string(),
                timezone: None,
                payload: None,
            },
        );

        // Should fail due to invalid cron expression
        let result = scheduler
            .add_schedule(&trigger, "invalid cron".to_string(), None, None)
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to create cron job")
        );

        // No job should be added
        assert_eq!(scheduler.active_job_count().await, 0);

        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_invalid_timezone() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(),
                timezone: Some("Invalid/Timezone".to_string()),
                payload: None,
            },
        );

        // Should fail due to invalid timezone
        let result = scheduler
            .add_schedule(
                &trigger,
                "0 0 * * * *".to_string(),
                Some("Invalid/Timezone".to_string()),
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid timezone"));

        // No job should be added
        assert_eq!(scheduler.active_job_count().await, 0);

        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_schedule_with_timezone() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(),
                timezone: Some("Asia/Shanghai".to_string()),
                payload: None,
            },
        );

        // Should succeed with valid timezone
        scheduler
            .add_schedule(
                &trigger,
                "0 0 * * * *".to_string(),
                Some("Asia/Shanghai".to_string()),
                None,
            )
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 1);

        // Clean up
        scheduler.remove_schedule(&trigger.id).await.unwrap();
        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_nonexistent_schedule() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        // Try to remove a schedule that doesn't exist
        let removed = scheduler
            .remove_schedule("nonexistent-trigger-id")
            .await
            .unwrap();

        // Should return false when no job was found
        assert!(!removed);

        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_active_job_count() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        // Initially no jobs
        assert_eq!(scheduler.active_job_count().await, 0);

        // Add first job
        let trigger1 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(),
                timezone: None,
                payload: None,
            },
        );

        scheduler
            .add_schedule(&trigger1, "0 0 * * * *".to_string(), None, None)
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 1);

        // Add second job
        let trigger2 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 30 * * * *".to_string(),
                timezone: None,
                payload: None,
            },
        );

        scheduler
            .add_schedule(&trigger2, "0 30 * * * *".to_string(), None, None)
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 2);

        // Remove first job
        scheduler.remove_schedule(&trigger1.id).await.unwrap();
        assert_eq!(scheduler.active_job_count().await, 1);

        // Remove second job
        scheduler.remove_schedule(&trigger2.id).await.unwrap();
        assert_eq!(scheduler.active_job_count().await, 0);

        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_schedules_same_workflow() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        // Add multiple schedules for the same workflow
        let trigger1 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(), // Every hour
                timezone: None,
                payload: None,
            },
        );

        let trigger2 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 30 * * * *".to_string(), // Every hour at 30 minutes
                timezone: None,
                payload: None,
            },
        );

        let trigger3 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 15 * * * *".to_string(), // Every hour at 15 minutes
                timezone: Some("America/New_York".to_string()),
                payload: None,
            },
        );

        // Add all three schedules
        scheduler
            .add_schedule(&trigger1, "0 0 * * * *".to_string(), None, None)
            .await
            .unwrap();

        scheduler
            .add_schedule(&trigger2, "0 30 * * * *".to_string(), None, None)
            .await
            .unwrap();

        scheduler
            .add_schedule(
                &trigger3,
                "0 15 * * * *".to_string(),
                Some("America/New_York".to_string()),
                None,
            )
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 3);

        // Remove each individually
        scheduler.remove_schedule(&trigger1.id).await.unwrap();
        assert_eq!(scheduler.active_job_count().await, 2);

        scheduler.remove_schedule(&trigger2.id).await.unwrap();
        assert_eq!(scheduler.active_job_count().await, 1);

        scheduler.remove_schedule(&trigger3.id).await.unwrap();
        assert_eq!(scheduler.active_job_count().await, 0);

        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_schedule_with_payload() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        let payload = serde_json::json!({
            "key": "value",
            "number": 42,
            "nested": {
                "field": "data"
            }
        });

        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(),
                timezone: None,
                payload: Some(payload.clone()),
            },
        );

        // Add schedule with custom payload
        scheduler
            .add_schedule(&trigger, "0 0 * * * *".to_string(), None, Some(payload))
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 1);

        // Clean up
        scheduler.remove_schedule(&trigger.id).await.unwrap();
        scheduler.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shutdown_prevents_new_schedules() {
        let (mut scheduler, _storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        // Add a job before shutdown
        let trigger1 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 0 * * * *".to_string(),
                timezone: None,
                payload: None,
            },
        );

        scheduler
            .add_schedule(&trigger1, "0 0 * * * *".to_string(), None, None)
            .await
            .unwrap();

        assert_eq!(scheduler.active_job_count().await, 1);

        // Shutdown the scheduler
        scheduler.shutdown().await.unwrap();

        // Try to add a new job after shutdown
        let trigger2 = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "0 30 * * * *".to_string(),
                timezone: None,
                payload: None,
            },
        );

        let result = scheduler
            .add_schedule(&trigger2, "0 30 * * * *".to_string(), None, None)
            .await;

        // Should fail because scheduler is shut down
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Scheduler has been shutdown")
        );

        // Job count should remain at 1 (only the first job)
        assert_eq!(scheduler.active_job_count().await, 1);
    }

    #[tokio::test]
    async fn test_schedule_execution_tracking() {
        let (mut scheduler, storage, _temp_dir) = setup_test_scheduler().await;

        scheduler.start().await.unwrap();

        let trigger = ActiveTrigger::new(
            "test-workflow".to_string(),
            TriggerConfig::Schedule {
                cron: "* * * * * *".to_string(), // Every second for testing
                timezone: None,
                payload: None,
            },
        );

        // Store the trigger in storage
        storage.triggers.activate_trigger(&trigger).unwrap();

        // Add schedule
        scheduler
            .add_schedule(&trigger, "* * * * * *".to_string(), None, None)
            .await
            .unwrap();

        // Get initial trigger count
        let initial_trigger = storage
            .triggers
            .get_active_trigger(&trigger.id)
            .unwrap()
            .unwrap();
        let initial_count = initial_trigger.trigger_count;

        // Wait for at least 2 full seconds to ensure at least one execution
        // (cron runs every second, so 2100ms guarantees at least one execution)
        tokio::time::sleep(tokio::time::Duration::from_millis(2100)).await;

        // Get updated trigger
        let updated_trigger = storage
            .triggers
            .get_active_trigger(&trigger.id)
            .unwrap()
            .unwrap();

        // Verify trigger count increased
        assert!(
            updated_trigger.trigger_count > initial_count,
            "Trigger count should have increased: {} vs {}",
            updated_trigger.trigger_count,
            initial_count
        );

        // Verify last_triggered_at was updated
        assert!(
            updated_trigger.last_triggered_at.is_some(),
            "Last triggered timestamp should be set"
        );

        // Clean up
        scheduler.remove_schedule(&trigger.id).await.unwrap();
        scheduler.shutdown().await.unwrap();
    }
}
