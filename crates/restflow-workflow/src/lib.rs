pub mod engine;
pub mod models;
pub mod node;
pub mod paths;
pub mod python;
pub mod services;
pub mod storage;

pub use models::*;

use engine::cron_scheduler::CronScheduler;
use engine::executor::WorkflowExecutor;
use engine::trigger_manager::TriggerManager;
use node::registry::NodeRegistry;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use storage::Storage;
use tracing::{error, info};

/// Core application state shared between server and Tauri modes
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub executor: Arc<WorkflowExecutor>,
    pub trigger_manager: Arc<TriggerManager>,
    pub cron_scheduler: Arc<CronScheduler>,
    pub python_manager: OnceCell<Arc<python::PythonManager>>,
    pub registry: Arc<NodeRegistry>,
}

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);

        let num_workers = storage.config.get_worker_count().unwrap_or(4);

        info!(num_workers, "Initializing RestFlow");

        let registry = Arc::new(NodeRegistry::new());

        let executor = Arc::new(WorkflowExecutor::new(
            storage.clone(),
            num_workers,
            registry.clone(),
        ));
        executor.start().await;

        let cron_scheduler = Arc::new(
            CronScheduler::new(storage.clone(), executor.clone())
                .await
                .map_err(|e| {
                    error!(error = %e, "Failed to create CronScheduler");
                    e
                })?,
        );

        if let Err(e) = cron_scheduler.start().await {
            error!(error = %e, "Failed to start CronScheduler");
        } else {
            info!("CronScheduler started successfully");
        }

        let trigger_manager = Arc::new(TriggerManager::new(
            storage.clone(),
            executor.clone(),
            cron_scheduler.clone(),
        ));

        if let Err(e) = trigger_manager.init().await {
            error!(error = %e, "Failed to initialize trigger manager");
        }

        Ok(Self {
            storage,
            executor,
            trigger_manager,
            cron_scheduler,
            python_manager: OnceCell::new(),
            registry,
        })
    }

    pub async fn get_python_manager(&self) -> anyhow::Result<Arc<python::PythonManager>> {
        if let Some(manager) = self.python_manager.get() {
            return Ok(manager.clone());
        }

        let manager = python::PythonManager::new().await?;

        let _ = self.python_manager.set(manager.clone());

        self.executor.set_python_manager(manager.clone()).await;

        Ok(self.python_manager.get().unwrap().clone())
    }

    pub fn is_python_ready(&self) -> bool {
        self.python_manager
            .get()
            .map(|m| m.is_ready())
            .unwrap_or(false)
    }
}
