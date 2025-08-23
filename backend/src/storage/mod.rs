pub mod queue;
pub mod workflow;

use redb::Database;
use std::sync::Arc;

pub use queue::{TaskQueue, TaskStatus, WorkflowTask};
pub use workflow::WorkflowStorage;

pub struct Storage {
    db: Arc<Database>,
    pub workflows: WorkflowStorage,
    pub queue: TaskQueue,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let db = Arc::new(Database::create(path)?);

        let write_txn = db.begin_write()?;
        write_txn.open_table(workflow::WORKFLOW_TABLE)?;
        write_txn.commit()?;

        let workflows = WorkflowStorage::new(db.clone());
        let queue = TaskQueue::new(db.clone())?;

        Ok(Self {
            db,
            workflows,
            queue,
        })
    }

    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
