use restflow_core::{AppCore, security::SecurityChecker};
use std::{ops::Deref, sync::Arc};

/// Application state shared across all API handlers
#[derive(Clone)]
pub struct AppState {
    pub core: Arc<AppCore>,
    pub security_checker: Arc<SecurityChecker>,
}

impl AppState {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            core,
            security_checker: Arc::new(SecurityChecker::with_defaults()),
        }
    }
}

impl Deref for AppState {
    type Target = Arc<AppCore>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}
