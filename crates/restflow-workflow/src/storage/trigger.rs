//! Typed trigger storage wrapper.

use crate::models::{ActiveTrigger, TriggerConfig};
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

/// Typed trigger storage wrapper around restflow-storage::TriggerStorage.
pub struct TriggerStorage {
    inner: restflow_storage::TriggerStorage,
}

impl TriggerStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::TriggerStorage::new(db)?,
        })
    }

    /// Activate trigger
    pub fn activate_trigger(&self, trigger: &ActiveTrigger) -> Result<()> {
        let json_bytes = serde_json::to_vec(trigger)?;
        self.inner.put_raw(&trigger.id, &json_bytes)
    }

    /// Deactivate trigger
    pub fn deactivate_trigger(&self, trigger_id: &str) -> Result<()> {
        self.inner.delete(trigger_id)?;
        Ok(())
    }

    /// Find active trigger by workflow_id
    pub fn get_active_trigger_by_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<ActiveTrigger>> {
        let triggers = self.inner.list_raw()?;
        for (_, bytes) in triggers {
            let trigger: ActiveTrigger = serde_json::from_slice(&bytes)?;
            if trigger.workflow_id == workflow_id {
                return Ok(Some(trigger));
            }
        }
        Ok(None)
    }

    /// Find active trigger by trigger_id
    pub fn get_active_trigger(&self, trigger_id: &str) -> Result<Option<ActiveTrigger>> {
        if let Some(bytes) = self.inner.get_raw(trigger_id)? {
            let trigger: ActiveTrigger = serde_json::from_slice(&bytes)?;
            Ok(Some(trigger))
        } else {
            Ok(None)
        }
    }

    /// Find workflow_id by webhook_id (trigger_id)
    pub fn get_workflow_by_webhook(&self, webhook_id: &str) -> Result<Option<String>> {
        if let Some(trigger) = self.get_active_trigger(webhook_id)? {
            Ok(Some(trigger.workflow_id))
        } else {
            Ok(None)
        }
    }

    /// Update trigger (record trigger count, etc.)
    pub fn update_trigger(&self, trigger: &ActiveTrigger) -> Result<()> {
        let json_bytes = serde_json::to_vec(trigger)?;
        self.inner.put_raw(&trigger.id, &json_bytes)
    }

    /// List all active triggers
    pub fn list_active_triggers(&self) -> Result<Vec<ActiveTrigger>> {
        let triggers = self.inner.list_raw()?;
        let mut result = Vec::new();
        for (_, bytes) in triggers {
            let trigger: ActiveTrigger = serde_json::from_slice(&bytes)?;
            result.push(trigger);
        }
        Ok(result)
    }

    /// Get all Schedule type triggers (for scheduler)
    pub fn list_schedule_triggers(&self) -> Result<Vec<ActiveTrigger>> {
        let triggers = self.list_active_triggers()?;
        Ok(triggers
            .into_iter()
            .filter(|t| matches!(t.trigger_config, TriggerConfig::Schedule { .. }))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AuthConfig;
    use tempfile::tempdir;

    fn create_test_webhook_trigger(id: &str, workflow_id: &str) -> ActiveTrigger {
        ActiveTrigger {
            id: id.to_string(),
            workflow_id: workflow_id.to_string(),
            trigger_config: TriggerConfig::Webhook {
                path: format!("/api/webhook/{}", id),
                method: "POST".to_string(),
                auth: Some(AuthConfig::ApiKey {
                    key: "test-key".to_string(),
                    header_name: Some("X-API-Key".to_string()),
                }),
            },
            trigger_count: 0,
            activated_at: chrono::Utc::now().timestamp(),
            last_triggered_at: None,
        }
    }

    fn create_test_schedule_trigger(id: &str, workflow_id: &str) -> ActiveTrigger {
        ActiveTrigger {
            id: id.to_string(),
            workflow_id: workflow_id.to_string(),
            trigger_config: TriggerConfig::Schedule {
                cron: "0 */5 * * * *".to_string(),
                timezone: Some("UTC".to_string()),
                payload: Some(serde_json::json!({"scheduled": true})),
            },
            trigger_count: 0,
            activated_at: chrono::Utc::now().timestamp(),
            last_triggered_at: None,
        }
    }

    fn setup_test_storage() -> (TriggerStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TriggerStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_activate_trigger() {
        let (storage, _temp_dir) = setup_test_storage();

        let trigger = create_test_webhook_trigger("trigger-001", "workflow-001");
        storage.activate_trigger(&trigger).unwrap();

        let retrieved = storage.get_active_trigger("trigger-001").unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "trigger-001");
        assert_eq!(retrieved.workflow_id, "workflow-001");
    }

    #[test]
    fn test_deactivate_trigger() {
        let (storage, _temp_dir) = setup_test_storage();

        let trigger = create_test_webhook_trigger("trigger-001", "workflow-001");
        storage.activate_trigger(&trigger).unwrap();

        storage.deactivate_trigger("trigger-001").unwrap();

        let retrieved = storage.get_active_trigger("trigger-001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_trigger_by_workflow() {
        let (storage, _temp_dir) = setup_test_storage();

        let trigger1 = create_test_webhook_trigger("trigger-001", "workflow-001");
        let trigger2 = create_test_webhook_trigger("trigger-002", "workflow-002");

        storage.activate_trigger(&trigger1).unwrap();
        storage.activate_trigger(&trigger2).unwrap();

        let found = storage
            .get_active_trigger_by_workflow("workflow-001")
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().workflow_id, "workflow-001");

        let not_found = storage
            .get_active_trigger_by_workflow("workflow-999")
            .unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_schedule_triggers() {
        let (storage, _temp_dir) = setup_test_storage();

        let webhook1 = create_test_webhook_trigger("webhook-001", "workflow-001");
        let schedule1 = create_test_schedule_trigger("schedule-001", "workflow-003");
        let schedule2 = create_test_schedule_trigger("schedule-002", "workflow-004");

        storage.activate_trigger(&webhook1).unwrap();
        storage.activate_trigger(&schedule1).unwrap();
        storage.activate_trigger(&schedule2).unwrap();

        let schedule_triggers = storage.list_schedule_triggers().unwrap();
        assert_eq!(schedule_triggers.len(), 2);

        let ids: Vec<String> = schedule_triggers.iter().map(|t| t.id.clone()).collect();
        assert!(ids.contains(&"schedule-001".to_string()));
        assert!(ids.contains(&"schedule-002".to_string()));
        assert!(!ids.contains(&"webhook-001".to_string()));
    }
}
