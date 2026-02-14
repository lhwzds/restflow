pub mod auth;
pub mod channel;
pub mod context;
pub mod daemon;
pub mod features;
pub mod hooks;
pub mod loader;
pub mod lsp;
pub mod mcp;
pub mod memory;
pub mod models;
pub mod output;
pub mod paths;
pub mod performance;
pub mod process;
pub mod prompt_files;
pub mod registry;
pub mod runtime;
pub mod security;
pub mod services;
pub mod skill_files;
pub mod steer;
pub mod storage;
pub mod tools;
pub mod utils;

pub use models::*;
pub use steer::SteerRegistry;

use std::sync::Arc;
use std::time::Duration;
use storage::Storage;
use tracing::{info, warn};

use crate::mcp::McpToolCache;

/// Core application state shared between server and Tauri modes
///
/// After AgentFlow refactor, this struct focuses on:
/// - Storage access for Agent, Skill, Trigger, and Secrets
/// - MCP tool cache management
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub mcp_tool_cache: Arc<McpToolCache>,
    pub features: Arc<features::Features>,
}

const DEFAULT_AGENT_NAME: &str = "Default Assistant";

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);
        prompt_files::ensure_prompt_templates()?;
        skill_files::ensure_default_skill_files()?;

        // Ensure default agent exists on first run
        Self::ensure_default_agent(&storage)?;
        if let Err(err) = storage.agents.reconcile_prompt_file_names() {
            warn!(
                error = %err,
                "Failed to reconcile agent prompt file names; continuing startup"
            );
        }
        match storage.agents.cleanup_orphan_prompt_files() {
            Ok(cleaned) => {
                if cleaned > 0 {
                    info!(cleaned, "Cleaned orphan agent prompt files");
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "Failed to cleanup orphan agent prompt files; continuing startup"
                );
            }
        }

        info!("Initializing RestFlow (Agent-centric mode)");

        let mcp_tool_cache = Arc::new(McpToolCache::new(Duration::from_secs(3600)));
        let config = storage.config.get_config()?.unwrap_or_default();
        let features = Arc::new(features::Features::from_config(&config));

        let core = Self {
            storage,
            mcp_tool_cache,
            features,
        };

        // Sync filesystem-backed default skills into database records.
        if let Ok(user_skills_dir) = paths::user_skills_dir() {
            let report = services::skill_sync::sync_all(&core, &user_skills_dir).await?;
            info!(
                scanned = report.scanned,
                created = report.created,
                updated = report.updated,
                skipped = report.skipped,
                failed = report.failed,
                "Default skills synchronized"
            );
            if report.failed > 0 {
                warn!(
                    failed = report.failed,
                    "Some skill files could not be loaded; check skill folder contents"
                );
            }
        }

        Ok(core)
    }

    /// Create default agent if no agents exist
    fn ensure_default_agent(storage: &Storage) -> anyhow::Result<()> {
        let agents = storage.agents.list_agents()?;
        if agents.is_empty() {
            info!("Creating default agent...");
            let agent_node = models::AgentNode::with_model(models::AIModel::CodexCli);
            let _created = storage
                .agents
                .create_agent(DEFAULT_AGENT_NAME.to_string(), agent_node)?;
            info!("Default agent created: {}", DEFAULT_AGENT_NAME);
        }
        Ok(())
    }
}
