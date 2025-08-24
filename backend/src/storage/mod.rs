pub mod task_queue;
pub mod workflow;
pub mod config;
pub mod trigger;

use redb::Database;
use std::sync::Arc;

pub use task_queue::TaskQueue;
pub use workflow::WorkflowStorage;
pub use config::{ConfigStorage, SystemConfig};
pub use trigger::TriggerStorage;

pub struct Storage {
    db: Arc<Database>,
    pub workflows: WorkflowStorage,
    pub queue: TaskQueue,
    pub config: ConfigStorage,
    pub triggers: TriggerStorage,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let db = Arc::new(Database::create(path)?);

        let write_txn = db.begin_write()?;
        write_txn.open_table(workflow::WORKFLOW_TABLE)?;
        write_txn.open_table(trigger::ACTIVE_TRIGGERS_TABLE)?;
        write_txn.commit()?;

        let workflows = WorkflowStorage::new(db.clone());
        let queue = TaskQueue::new(db.clone())?;
        let config = ConfigStorage::new(db.clone());
        let triggers = TriggerStorage::new(db.clone());
        
        // Initialize config table and defaults
        config.init()?;

        Ok(Self {
            db,
            workflows,
            queue,
            config,
            triggers,
        })
    }

    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
