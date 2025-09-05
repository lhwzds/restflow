pub mod models;
pub mod engine;
pub mod storage;
pub mod node;
pub mod tools;
pub mod api;
pub mod services;

pub use models::*;

use engine::executor::WorkflowExecutor;
use engine::trigger_manager::TriggerManager;
use storage::Storage;
use std::sync::Arc;

/// Core application state shared between server and Tauri modes
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub executor: Arc<WorkflowExecutor>,
    pub trigger_manager: Arc<TriggerManager>,
}

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);
        
        // Get worker count from database configuration
        let num_workers = storage.config.get_worker_count().unwrap_or(4);
        
        println!("Initializing RestFlow with {} workers", num_workers);
        
        // Create and start workflow executor
        let executor = Arc::new(WorkflowExecutor::new_async(
            storage.clone(),
            num_workers
        ));
        executor.start().await;
        
        // Create trigger manager
        let trigger_manager = Arc::new(TriggerManager::new(
            storage.clone(),
            executor.clone()
        ));
        
        // Initialize trigger manager
        if let Err(e) = trigger_manager.init().await {
            eprintln!("Failed to initialize trigger manager: {}", e);
        }
        
        Ok(Self {
            storage,
            executor,
            trigger_manager,
        })
    }
}