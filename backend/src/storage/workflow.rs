use crate::models::Workflow;
use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

pub const WORKFLOW_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("workflow");

pub struct WorkflowStorage {
    db: Arc<Database>,
}

impl WorkflowStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table if not exists
        let write_txn = db.begin_write()?;
        write_txn.open_table(WORKFLOW_TABLE)?;
        write_txn.commit()?;
        
        Ok(Self { db })
    }

    pub fn create_workflow(&self, workflow: &Workflow) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;
            let json_bytes = serde_json::to_vec(workflow)?;
            table.insert(workflow.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Workflow> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(WORKFLOW_TABLE)?;

        if let Some(value) = table.get(id)? {
            let workflow: Workflow = serde_json::from_slice(value.value())?;
            Ok(workflow)
        } else {
            Err(anyhow::anyhow!("Workflow {} not found", id))
        }
    }

    pub fn list_workflows(&self) -> Result<Vec<Workflow>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(WORKFLOW_TABLE)?;

        let mut workflows = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let workflow: Workflow = serde_json::from_slice(value.value())?;
            workflows.push(workflow);
        }

        Ok(workflows)
    }

    pub fn update_workflow(
        &self,
        id: &str,
        workflow: &Workflow,
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            if table.get(id)?.is_none() {
                return Err(anyhow::anyhow!("Workflow not found"));
            }

            let json_bytes = serde_json::to_vec(workflow)?;
            table.insert(id, json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn delete_workflow(&self, id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            if table.get(id)?.is_none() {
                return Err(anyhow::anyhow!("Workflow not found"));
            }

            table.remove(id)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}