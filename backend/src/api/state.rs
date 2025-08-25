use crate::engine::executor::WorkflowExecutor;
use crate::engine::trigger_manager::TriggerManager;
use crate::storage::Storage;
use std::sync::Arc;

/// Application state shared across all API handlers
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub executor: Arc<WorkflowExecutor>,
    pub trigger_manager: Arc<TriggerManager>,
}

impl AppState {
    pub fn new(
        storage: Arc<Storage>,
        executor: Arc<WorkflowExecutor>,
        trigger_manager: Arc<TriggerManager>,
    ) -> Self {
        Self {
            storage,
            executor,
            trigger_manager,
        }
    }
}