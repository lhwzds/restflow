pub mod task_queue;
pub mod workflow;
pub mod config;
pub mod trigger;

use anyhow::Result;
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
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);

        let workflows = WorkflowStorage::new(db.clone())?;
        let queue = TaskQueue::new(db.clone())?;
        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;

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
