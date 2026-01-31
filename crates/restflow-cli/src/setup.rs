//! CLI setup module
//!
//! Handles initialization of the RestFlow core for CLI usage.

use anyhow::Result;
use restflow_core::{AppCore, paths};
use std::sync::Arc;

use crate::config::CliConfig;

/// Build the embedded RestFlow core
pub async fn prepare_core(config: &CliConfig) -> Result<Arc<AppCore>> {
    init_core(config).await
}

async fn init_core(config: &CliConfig) -> Result<Arc<AppCore>> {
    let db_path = config
        .default
        .db_path
        .clone()
        .unwrap_or(paths::ensure_database_path_string()?);
    Ok(Arc::new(AppCore::new(&db_path).await?))
}

// TODO: Add API key validation when agent execution is re-enabled
// The old validation used rig-core which has been removed.
// Agent execution is now handled by restflow-ai's AgentExecutor.
