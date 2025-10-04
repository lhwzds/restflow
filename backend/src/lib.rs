pub mod models;
pub mod engine;
pub mod storage;
pub mod node;
pub mod tools;
pub mod api;
pub mod services;
pub mod python;

pub use models::*;

use engine::executor::WorkflowExecutor;
use engine::trigger_manager::TriggerManager;
use node::registry::NodeRegistry;
use storage::Storage;
use std::sync::Arc;
use once_cell::sync::OnceCell;
use tracing::{info, error};

/// Core application state shared between server and Tauri modes
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub executor: Arc<WorkflowExecutor>,
    pub trigger_manager: Arc<TriggerManager>,
    pub python_manager: OnceCell<Arc<python::PythonManager>>,
    pub registry: Arc<NodeRegistry>,
}

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);

        // Get worker count from database configuration
        let num_workers = storage.config.get_worker_count().unwrap_or(4);

        info!(num_workers, "Initializing RestFlow");

        // Create node registry
        let registry = Arc::new(NodeRegistry::new());

        // Create and start workflow executor
        let executor = Arc::new(WorkflowExecutor::new_async(
            storage.clone(),
            num_workers,
            registry.clone()
        ));
        executor.start().await;

        // Create trigger manager
        let trigger_manager = Arc::new(TriggerManager::new(
            storage.clone(),
            executor.clone(),
            registry.clone()
        ));

        // Initialize trigger manager
        if let Err(e) = trigger_manager.init().await {
            error!(error = %e, "Failed to initialize trigger manager");
        }

        Ok(Self {
            storage,
            executor,
            trigger_manager,
            python_manager: OnceCell::new(),
            registry,
        })
    }
    
    /// Get or initialize Python manager
    pub async fn get_python_manager(&self) -> anyhow::Result<Arc<python::PythonManager>> {
        if let Some(manager) = self.python_manager.get() {
            return Ok(manager.clone());
        }
        
        // Initialize Python manager lazily
        let manager = python::PythonManager::new().await?;
        
        // Try to set it, but if another thread already set it, use that one
        let _ = self.python_manager.set(manager.clone());
        
        Ok(self.python_manager.get().unwrap().clone())
    }
    
    /// Check if Python is available
    pub fn is_python_ready(&self) -> bool {
        self.python_manager.get().map(|m| m.is_ready()).unwrap_or(false)
    }
}