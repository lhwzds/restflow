use anyhow::Result;
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager};
use restflow_core::paths;
use restflow_core::{AppCore, security::SecurityChecker};
use restflow_storage::AuthProfileStorage;
use std::{ops::Deref, sync::Arc};
use tracing::warn;

/// Application state shared across all API handlers
#[derive(Clone)]
pub struct AppState {
    pub core: Arc<AppCore>,
    pub security_checker: Arc<SecurityChecker>,
    pub auth_manager: Arc<AuthProfileManager>,
}

impl AppState {
    /// Create AppState with initialized AuthProfileManager
    pub async fn new(core: Arc<AppCore>) -> Result<Self> {
        let secrets = Arc::new(core.storage.secrets.clone());
        let db = core.storage.get_db();
        let storage = AuthProfileStorage::new(db)?;

        let config = AuthManagerConfig::default();
        let auth_manager = Arc::new(AuthProfileManager::with_storage(config, secrets, Some(storage)));

        // Migrate from old JSON format if exists
        if let Ok(data_dir) = paths::ensure_restflow_dir() {
            let old_json = data_dir.join("auth_profiles.json");
            if let Err(e) = auth_manager.migrate_from_json(&old_json).await {
                warn!(error = %e, "Failed to migrate auth profiles from JSON");
            }
        }

        auth_manager.initialize().await?;
        auth_manager.discover().await?;

        Ok(Self {
            core,
            security_checker: Arc::new(SecurityChecker::with_defaults()),
            auth_manager,
        })
    }

    /// Create AppState synchronously (for tests that don't need auth)
    #[allow(dead_code)]
    pub fn new_sync(core: Arc<AppCore>) -> Self {
        let secrets = Arc::new(core.storage.secrets.clone());
        let config = AuthManagerConfig::default();
        let auth_manager = Arc::new(AuthProfileManager::with_config(config, secrets));

        Self {
            core,
            security_checker: Arc::new(SecurityChecker::with_defaults()),
            auth_manager,
        }
    }
}

impl Deref for AppState {
    type Target = Arc<AppCore>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
