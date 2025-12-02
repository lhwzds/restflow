use restflow_workflow::AppCore;
use std::sync::Arc;

/// Application state shared across all API handlers
/// This is now just an alias for AppCore to avoid duplication
pub type AppState = Arc<AppCore>;
