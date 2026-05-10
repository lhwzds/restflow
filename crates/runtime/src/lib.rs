pub mod agent_validation;
pub mod auth;
pub mod config;
pub mod daemon;
mod encryption;
pub mod features;
pub mod paths;
pub mod process;
pub mod prompt_files;
mod provider_policy;
pub mod runtime;
pub mod secrets;
pub mod services;
pub mod session_log;
pub mod steer;
pub mod storage;
#[cfg(test)]
pub(crate) mod test_support;
pub mod time_utils;
pub mod tools;

pub use config::{
    AgentDefaults, AgentSettings, ApiDefaults, ApiSettings, CliConfig, ConfigDocument,
    ConfigSourcePathInfo, ConfigStorage, ConfigValueSourceInfo, ConfigValueSourceKind,
    EffectiveConfigSources, RegistryDefaults, RegistrySettings, RuntimeDefaults, RuntimeSettings,
    SystemConfig, SystemSection, effective_config_sources, load_cli_config, load_global_cli_config,
    write_cli_config,
};
pub use secrets::{Secret, SecretStorage, SecretStorageConfig};
pub use services::agent_catalog::{AgentStorage, DEFAULT_ASSISTANT_NAME, StoredAgent};
pub use steer::SteerRegistry;
pub use types::{
    AgentMeta, AgentNode, AgentType, ApiKeyConfig, ChatExecutionStatus, ChatMessage, ChatRole,
    ChatSession, ChatSessionMetadata, ChatSessionSource, ChatSessionSummary, ChatSessionUpdate,
    ChildRunListQuery, CodexCliExecutionMode, ExecutionContainerKind, ExecutionContainerRef,
    ExecutionContainerSummary, ExecutionStepInfo, ExecutionThread, MessageExecution, ModelId,
    ModelMetadataDTO, ModelRoutingConfig, Provider, RunArtifact, RunArtifactKind, RunKind,
    RunListQuery, RunSummary, RunTimeline, Skill, SkillGating, SkillMeta, SkillReference,
    SkillScript, SkillSource, SkillStatus, SteerMessage, SteerSource, ValidationError,
    ValidationErrorResponse, encode_validation_error,
};

use std::sync::Arc;
use storage::Storage;
use tracing::{info, warn};

/// Core application state shared between daemon-backed application modes
///
/// AppCore wires together local state, features, and runtime services.
pub struct AppCore {
    pub storage: Arc<Storage>,
    pub features: Arc<features::Features>,
}

impl AppCore {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::new(db_path)?);
        prompt_files::ensure_prompt_templates()?;

        // Ensure default agent exists on first run
        Self::ensure_default_agent(&storage)?;
        if let Err(err) = storage.agents.reconcile_prompt_file_names() {
            warn!(
                error = %err,
                "Failed to reconcile agent prompt file names; continuing startup"
            );
        }
        info!("Initializing RestFlow (Agent-centric mode)");

        let config = storage.config.get_effective_config()?;
        let features = Arc::new(features::Features::from_config(&config));

        let core = Self { storage, features };

        Ok(core)
    }

    /// Create default agent if no agents exist
    fn ensure_default_agent(storage: &Storage) -> anyhow::Result<()> {
        let agents = storage.agents.list_agents()?;
        if agents.is_empty() {
            info!("Creating default agent...");
            let agent_node = types::AgentNode::with_model(types::ModelId::CodexCli);
            let _created = storage
                .agents
                .create_agent(DEFAULT_ASSISTANT_NAME.to_string(), agent_node)?;
            info!("Default agent created: {}", DEFAULT_ASSISTANT_NAME);
        }
        Ok(())
    }
}
