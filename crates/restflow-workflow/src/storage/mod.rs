pub mod agent;
pub mod config;
pub mod execution_history;
pub mod secrets;
pub mod task_queue;
pub mod trigger;
pub mod workflow;

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

pub use agent::AgentStorage;
pub use config::{ConfigStorage, SystemConfig};
pub use execution_history::ExecutionHistoryStorage;
pub use secrets::SecretStorage;
pub use task_queue::TaskQueue;
pub use trigger::TriggerStorage;
pub use workflow::WorkflowStorage;

pub struct Storage {
    db: Arc<Database>,
    pub workflows: WorkflowStorage,
    pub queue: TaskQueue,
    pub config: ConfigStorage,
    pub triggers: TriggerStorage,
    pub agents: AgentStorage,
    pub secrets: SecretStorage,
    pub execution_history: ExecutionHistoryStorage,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(Database::create(path)?);

        let workflows = WorkflowStorage::new(db.clone())?;
        let queue = TaskQueue::new(db.clone())?;
        let config = ConfigStorage::new(db.clone())?;
        let triggers = TriggerStorage::new(db.clone())?;
        let agents = AgentStorage::new(db.clone())?;
        let secrets = SecretStorage::new(db.clone())?;
        let execution_history = ExecutionHistoryStorage::new(db.clone())?;

        Ok(Self {
            db,
            workflows,
            queue,
            config,
            triggers,
            agents,
            secrets,
            execution_history,
        })
    }

    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
