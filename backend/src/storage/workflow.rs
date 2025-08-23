use crate::models::Workflow;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

pub const WORKFLOW_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("workflow");

pub struct WorkflowStorage {
    db: Arc<Database>,
}

impl WorkflowStorage {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn create_workflow(&self, workflow: &Workflow) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;
            let json_bytes = serde_json::to_vec(workflow)?;
            table.insert(workflow.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Option<Workflow>, Box<dyn std::error::Error + Send + Sync>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(WORKFLOW_TABLE)?;

        if let Some(value) = table.get(id)? {
            let workflow: Workflow = serde_json::from_slice(value.value())?;
            Ok(Some(workflow))
        } else {
            Ok(None)
        }
    }

    pub fn list_workflows(&self) -> Result<Vec<Workflow>, Box<dyn std::error::Error + Send + Sync>> {
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            if table.get(id)?.is_none() {
                return Err("Workflow not found".into());
            }

            let json_bytes = serde_json::to_vec(workflow)?;
            table.insert(id, json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn delete_workflow(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            if table.get(id)?.is_none() {
                return Err("Workflow not found".into());
            }

            table.remove(id)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}