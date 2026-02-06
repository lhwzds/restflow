//! CLI setup module
//!
//! Handles initialization of the RestFlow core for CLI usage.

use anyhow::Result;
use restflow_core::{AppCore, paths};
use std::sync::Arc;

/// Resolve the database path for CLI usage.
pub fn resolve_db_path(db_path: Option<String>) -> Result<String> {
    match db_path {
        Some(path) => Ok(path),
        None => paths::ensure_database_path_string(),
    }
}

/// Build the embedded RestFlow core
pub async fn prepare_core(db_path: Option<String>) -> Result<Arc<AppCore>> {
    let db_path = resolve_db_path(db_path)?;
    Ok(Arc::new(AppCore::new(&db_path).await?))
}

// TODO: Add API key validation when agent execution is re-enabled
// The old validation used rig-core which has been removed.
// Agent execution is now handled by restflow-ai's AgentExecutor.
