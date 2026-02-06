pub mod auth;
pub mod channel;
pub mod context;
pub mod daemon;
pub mod engine;
pub mod loader;
pub mod lsp;
pub mod mcp;
pub mod memory;
pub mod models;
pub mod node;
pub mod paths;
pub mod performance;
pub mod process;
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

/// Default agent prompt embedded at compile time
const DEFAULT_AGENT_PROMPT: &str = include_str!("../assets/default_agent.md");
const DEFAULT_AGENT_NAME: &str = "Default Assistant";

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);

        // Ensure default agent exists on first run
        Self::ensure_default_agent(&storage)?;

        info!("Initializing RestFlow (Agent-centric mode)");

        Ok(Self {
            storage,
            python_manager: OnceCell::new(),
        })
    }

    /// Create default agent if no agents exist
    fn ensure_default_agent(storage: &Storage) -> anyhow::Result<()> {
        let agents = storage.agents.list_agents()?;
        if agents.is_empty() {
            info!("Creating default agent...");
            let agent_node = models::AgentNode::new().with_prompt(DEFAULT_AGENT_PROMPT);
            storage
                .agents
                .create_agent(DEFAULT_AGENT_NAME.to_string(), agent_node)?;
            info!("Default agent created: {}", DEFAULT_AGENT_NAME);
        }
        Ok(())
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
