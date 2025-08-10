use crate::core::workflow::Workflow;
use redb::{Database, Error, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const WORKFLOW_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("workflow");

pub struct WorkflowStorage {
    db: Arc<Database>,
}

impl WorkflowStorage {
    pub fn new(path: &str) -> Result<Self, redb::Error> {
        let db = Database::create(path)?;

        let write_txn = db.begin_write()?;

        let _ = write_txn.open_table(WORKFLOW_TABLE)?;

        write_txn.commit()?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn add_workflow(&self, workflow: &Workflow) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            let json_bytes = serde_json::to_vec(workflow)?;

            table.insert(workflow.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Option<Workflow>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(WORKFLOW_TABLE)?;

        if let Some(value) = table.get(id)? {
            let workflow: Workflow = serde_json::from_slice(value.value())?;

            Ok(Some(workflow))
        } else {
            Ok(None)
        }
    }
}
