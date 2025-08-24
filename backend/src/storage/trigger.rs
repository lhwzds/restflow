use crate::models::ActiveTrigger;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;
use anyhow::Result;

// Store active triggers
pub const ACTIVE_TRIGGERS_TABLE: TableDefinition<&str, &[u8]> = 
    TableDefinition::new("active_triggers");

pub struct TriggerStorage {
    db: Arc<Database>,
}

impl TriggerStorage {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
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
    pub fn get_active_trigger_by_workflow(&self, workflow_id: &str) -> Result<Option<ActiveTrigger>> {
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
        Ok(triggers.into_iter()
            .filter(|t| matches!(t.trigger_config, crate::models::TriggerConfig::Schedule { .. }))
            .collect())
    }
}