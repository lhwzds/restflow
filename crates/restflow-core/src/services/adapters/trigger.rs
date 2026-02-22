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
