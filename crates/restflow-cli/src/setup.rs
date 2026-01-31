//! CLI setup module
//!
//! Handles initialization of the RestFlow core for CLI usage.

use anyhow::Result;
use restflow_core::{paths, AppCore};
use std::sync::Arc;

/// Build the embedded RestFlow core
pub async fn prepare_core(db_path: Option<String>) -> Result<Arc<AppCore>> {
    let db_path = match db_path {
        Some(path) => path,
        None => paths::ensure_database_path_string()?,
    };
    Ok(Arc::new(AppCore::new(&db_path).await?))
}

// TODO: Add API key validation when agent execution is re-enabled
// The old validation used rig-core which has been removed.
// Agent execution is now handled by restflow-ai's AgentExecutor.
