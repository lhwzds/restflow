//! Application state management for Tauri

use restflow_workflow::AppCore;
use std::sync::Arc;

/// Application state shared across Tauri commands
pub struct AppState {
    pub core: Arc<AppCore>,
}

impl AppState {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let core = Arc::new(AppCore::new(db_path).await?);
        Ok(Self { core })
    }
}
