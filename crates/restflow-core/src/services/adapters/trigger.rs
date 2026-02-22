//! TriggerStore adapter backed by TriggerStorage.

use crate::models::TriggerConfig;
use crate::storage::TriggerStorage;
use restflow_ai::tools::TriggerStore;
use restflow_tools::ToolError;
use serde_json::{Value, json};

pub struct TriggerStoreAdapter {
    storage: TriggerStorage,
}

impl TriggerStoreAdapter {
    pub fn new(storage: TriggerStorage) -> Self {
        Self { storage }
    }
}

impl TriggerStore for TriggerStoreAdapter {
    fn create_trigger(
        &self,
        workflow_id: &str,
        config: Value,
        id: Option<&str>,
    ) -> restflow_tools::Result<Value> {
        let trigger_config: TriggerConfig =
            serde_json::from_value(config).map_err(|e| ToolError::Tool(e.to_string()))?;
        let mut trigger =
            crate::models::ActiveTrigger::new(workflow_id.to_string(), trigger_config);
        if let Some(id) = id {
            trigger.id = id.to_string();
        }
        self.storage
            .activate_trigger(&trigger)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(serde_json::to_value(trigger)?)
    }

    fn list_triggers(&self) -> restflow_tools::Result<Value> {
        let triggers = self
            .storage
            .list_active_triggers()
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(serde_json::to_value(triggers)?)
    }

    fn delete_trigger(&self, id: &str) -> restflow_tools::Result<Value> {
        self.storage
            .deactivate_trigger(id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({ "id": id, "deleted": true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::tools::TriggerStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (TriggerStoreAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = TriggerStorage::new(db).unwrap();
        (TriggerStoreAdapter::new(storage), temp_dir)
    }

    #[test]
    fn test_create_and_list_trigger() {
        let (adapter, _dir) = setup();
        let config = json!({ "type": "schedule", "cron": "0 * * * *" });
        let result = adapter.create_trigger("wf-1", config, Some("trig-1")).unwrap();
        assert_eq!(result["id"], "trig-1");

        let list = adapter.list_triggers().unwrap();
        let triggers = list.as_array().unwrap();
        assert_eq!(triggers.len(), 1);
    }

    #[test]
    fn test_delete_trigger() {
        let (adapter, _dir) = setup();
        let config = json!({ "type": "schedule", "cron": "0 * * * *" });
        adapter.create_trigger("wf-1", config, Some("trig-del")).unwrap();

        let result = adapter.delete_trigger("trig-del").unwrap();
        assert_eq!(result["deleted"], true);

        let list = adapter.list_triggers().unwrap();
        let triggers = list.as_array().unwrap();
        assert_eq!(triggers.len(), 0);
    }

    #[test]
    fn test_list_triggers_empty() {
        let (adapter, _dir) = setup();
        let list = adapter.list_triggers().unwrap();
        let triggers = list.as_array().unwrap();
        assert!(triggers.is_empty());
    }
}
