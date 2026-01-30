pub mod channel;
pub mod engine;
pub mod memory;
pub mod models;
pub mod node;
pub mod paths;
pub mod python;
pub mod registry;
pub mod security;
pub mod services;
pub mod storage;

pub use models::*;

use once_cell::sync::OnceCell;
use std::sync::Arc;
use storage::Storage;
use tracing::info;

/// Core application state shared between server and Tauri modes
///
/// After AgentFlow refactor, this struct focuses on:
/// - Storage access for Agent, Skill, Trigger, and Secrets
/// - Python runtime management for PythonTool
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub python_manager: OnceCell<Arc<python::PythonManager>>,
}

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);

        info!("Initializing RestFlow (Agent-centric mode)");

        Ok(Self {
            storage,
            python_manager: OnceCell::new(),
        })
    }

    pub async fn get_python_manager(&self) -> anyhow::Result<Arc<python::PythonManager>> {
        if let Some(manager) = self.python_manager.get() {
            return Ok(manager.clone());
        }

        let manager = python::PythonManager::new().await?;
        let _ = self.python_manager.set(manager.clone());

        Ok(self.python_manager.get().unwrap().clone())
    }

    pub fn is_python_ready(&self) -> bool {
        self.python_manager
            .get()
            .map(|m| m.is_ready())
            .unwrap_or(false)
    }
}
