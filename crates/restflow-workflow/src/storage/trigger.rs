use crate::models::ActiveTrigger;
use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

// Store active triggers
pub const ACTIVE_TRIGGERS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("active_triggers");

pub struct TriggerStorage {
    db: Arc<Database>,
}

impl TriggerStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table if not exists
        let write_txn = db.begin_write()?;
        write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    // Activate trigger
    pub fn activate_trigger(&self, trigger: &ActiveTrigger) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
            let json_bytes = serde_json::to_vec(trigger)?;
            table.insert(trigger.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Deactivate trigger
    pub fn deactivate_trigger(&self, trigger_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
            table.remove(trigger_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Find active trigger by workflow_id
    pub fn get_active_trigger_by_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<ActiveTrigger>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;

        for item in table.iter()? {
            let (_, value) = item?;
            let trigger: ActiveTrigger = serde_json::from_slice(value.value())?;
            if trigger.workflow_id == workflow_id {
                return Ok(Some(trigger));
            }
        }

        Ok(None)
    }

    // Find active trigger by trigger_id
    pub fn get_active_trigger(&self, trigger_id: &str) -> Result<Option<ActiveTrigger>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;

        if let Some(value) = table.get(trigger_id)? {
            let trigger: ActiveTrigger = serde_json::from_slice(value.value())?;
            Ok(Some(trigger))
        } else {
            Ok(None)
        }
    }

    // Find workflow_id by webhook_id (trigger_id)
    pub fn get_workflow_by_webhook(&self, webhook_id: &str) -> Result<Option<String>> {
        if let Some(trigger) = self.get_active_trigger(webhook_id)? {
            Ok(Some(trigger.workflow_id))
        } else {
            Ok(None)
        }
    }

    // Update trigger (record trigger count, etc.)
    pub fn update_trigger(&self, trigger: &ActiveTrigger) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;
            let json_bytes = serde_json::to_vec(trigger)?;
            table.insert(trigger.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // List all active triggers
    pub fn list_active_triggers(&self) -> Result<Vec<ActiveTrigger>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ACTIVE_TRIGGERS_TABLE)?;

        let mut triggers = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let trigger: ActiveTrigger = serde_json::from_slice(value.value())?;
            triggers.push(trigger);
        }

        Ok(triggers)
    }

    // Get all Schedule type triggers (for scheduler)
    pub fn list_schedule_triggers(&self) -> Result<Vec<ActiveTrigger>> {
        let triggers = self.list_active_triggers()?;
        Ok(triggers
            .into_iter()
            .filter(|t| {
                matches!(
                    t.trigger_config,
                    crate::models::TriggerConfig::Schedule { .. }
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActiveTrigger, AuthConfig, TriggerConfig};
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

        // Verify it was activated
        let retrieved = storage.get_active_trigger("trigger-001").unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "trigger-001");
        assert_eq!(retrieved.workflow_id, "workflow-001");
    }

    #[test]
    fn test_deactivate_trigger() {
        let (storage, _temp_dir) = setup_test_storage();

        // First activate
        let trigger = create_test_webhook_trigger("trigger-001", "workflow-001");
        storage.activate_trigger(&trigger).unwrap();

        // Then deactivate
        storage.deactivate_trigger("trigger-001").unwrap();

        // Verify it was deactivated
        let retrieved = storage.get_active_trigger("trigger-001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_active_trigger() {
        let (storage, _temp_dir) = setup_test_storage();

        let trigger = create_test_webhook_trigger("trigger-001", "workflow-001");
        storage.activate_trigger(&trigger).unwrap();

        let retrieved = storage.get_active_trigger("trigger-001").unwrap();
        assert!(retrieved.is_some());

        // Test non-existent trigger
        let non_existent = storage.get_active_trigger("nonexistent").unwrap();
        assert!(non_existent.is_none());
    }

    #[test]
    fn test_get_trigger_by_workflow() {
        let (storage, _temp_dir) = setup_test_storage();

        // Activate multiple triggers for different workflows
        let trigger1 = create_test_webhook_trigger("trigger-001", "workflow-001");
        let trigger2 = create_test_webhook_trigger("trigger-002", "workflow-002");
        let trigger3 = create_test_schedule_trigger("trigger-003", "workflow-001");

        storage.activate_trigger(&trigger1).unwrap();
        storage.activate_trigger(&trigger2).unwrap();
        storage.activate_trigger(&trigger3).unwrap();

        // Find trigger by workflow_id (returns first match)
        let found = storage
            .get_active_trigger_by_workflow("workflow-001")
            .unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.workflow_id, "workflow-001");

        // Test non-existent workflow
        let not_found = storage
            .get_active_trigger_by_workflow("workflow-999")
            .unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_get_workflow_by_webhook() {
        let (storage, _temp_dir) = setup_test_storage();

        let trigger = create_test_webhook_trigger("webhook-001", "workflow-001");
        storage.activate_trigger(&trigger).unwrap();

        // Get workflow_id by webhook_id
        let workflow_id = storage.get_workflow_by_webhook("webhook-001").unwrap();
        assert!(workflow_id.is_some());
        assert_eq!(workflow_id.unwrap(), "workflow-001");

        // Test non-existent webhook
        let not_found = storage.get_workflow_by_webhook("nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_update_trigger() {
        let (storage, _temp_dir) = setup_test_storage();

        // Create and activate trigger
        let mut trigger = create_test_webhook_trigger("trigger-001", "workflow-001");
        storage.activate_trigger(&trigger).unwrap();

        // Update trigger (increment count, set last_triggered_at)
        trigger.trigger_count = 5;
        trigger.last_triggered_at = Some(chrono::Utc::now().timestamp());
        storage.update_trigger(&trigger).unwrap();

        // Verify update
        let retrieved = storage.get_active_trigger("trigger-001").unwrap().unwrap();
        assert_eq!(retrieved.trigger_count, 5);
        assert!(retrieved.last_triggered_at.is_some());
    }

    #[test]
    fn test_list_active_triggers() {
        let (storage, _temp_dir) = setup_test_storage();

        // Activate multiple triggers
        let trigger1 = create_test_webhook_trigger("trigger-001", "workflow-001");
        let trigger2 = create_test_webhook_trigger("trigger-002", "workflow-002");
        let trigger3 = create_test_schedule_trigger("trigger-003", "workflow-003");

        storage.activate_trigger(&trigger1).unwrap();
        storage.activate_trigger(&trigger2).unwrap();
        storage.activate_trigger(&trigger3).unwrap();

        // List all triggers
        let triggers = storage.list_active_triggers().unwrap();
        assert_eq!(triggers.len(), 3);

        let ids: Vec<String> = triggers.iter().map(|t| t.id.clone()).collect();
        assert!(ids.contains(&"trigger-001".to_string()));
        assert!(ids.contains(&"trigger-002".to_string()));
        assert!(ids.contains(&"trigger-003".to_string()));
    }

    #[test]
    fn test_list_schedule_triggers() {
        let (storage, _temp_dir) = setup_test_storage();

        // Activate mixed trigger types
        let webhook1 = create_test_webhook_trigger("webhook-001", "workflow-001");
        let webhook2 = create_test_webhook_trigger("webhook-002", "workflow-002");
        let schedule1 = create_test_schedule_trigger("schedule-001", "workflow-003");
        let schedule2 = create_test_schedule_trigger("schedule-002", "workflow-004");

        storage.activate_trigger(&webhook1).unwrap();
        storage.activate_trigger(&webhook2).unwrap();
        storage.activate_trigger(&schedule1).unwrap();
        storage.activate_trigger(&schedule2).unwrap();

        // List only schedule triggers
        let schedule_triggers = storage.list_schedule_triggers().unwrap();
        assert_eq!(schedule_triggers.len(), 2);

        let ids: Vec<String> = schedule_triggers.iter().map(|t| t.id.clone()).collect();
        assert!(ids.contains(&"schedule-001".to_string()));
        assert!(ids.contains(&"schedule-002".to_string()));
        assert!(!ids.contains(&"webhook-001".to_string()));
    }

    #[test]
    fn test_trigger_persistence() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create and activate triggers
        {
            let db = Arc::new(Database::create(&db_path).unwrap());
            let storage = TriggerStorage::new(db).unwrap();

            let trigger1 = create_test_webhook_trigger("trigger-001", "workflow-001");
            let trigger2 = create_test_schedule_trigger("trigger-002", "workflow-002");

            storage.activate_trigger(&trigger1).unwrap();
            storage.activate_trigger(&trigger2).unwrap();
        }

        // Open database again and verify triggers persisted
        {
            let db = Arc::new(Database::open(&db_path).unwrap());
            let storage = TriggerStorage::new(db).unwrap();

            let triggers = storage.list_active_triggers().unwrap();
            assert_eq!(triggers.len(), 2);

            let trigger1 = storage.get_active_trigger("trigger-001").unwrap();
            assert!(trigger1.is_some());
            assert_eq!(trigger1.unwrap().workflow_id, "workflow-001");

            let trigger2 = storage.get_active_trigger("trigger-002").unwrap();
            assert!(trigger2.is_some());
            assert_eq!(trigger2.unwrap().workflow_id, "workflow-002");
        }
    }
}
