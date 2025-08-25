use crate::engine::executor::AsyncWorkflowExecutor;
use crate::engine::trigger_manager::TriggerManager;
use crate::storage::Storage;
use std::sync::Arc;

/// Application state shared across all API handlers
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub executor: Arc<AsyncWorkflowExecutor>,
    pub trigger_manager: Arc<TriggerManager>,
}

impl AppState {
    pub fn new(
        storage: Arc<Storage>,
        executor: Arc<AsyncWorkflowExecutor>,
        trigger_manager: Arc<TriggerManager>,
    ) -> Self {
        Self {
            storage,
            executor,
            trigger_manager,
        }
    }
}