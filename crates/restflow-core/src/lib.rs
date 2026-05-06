pub mod auth;
pub(crate) mod boundary;
pub mod daemon;
pub mod features;
pub mod loader;
pub mod lsp;
pub mod mcp;
pub mod models;
pub mod paths;
pub mod performance;
pub mod process;
pub mod prompt_files;
pub mod registry;
pub mod runtime;
pub mod security;
pub mod services;
pub mod session_import;
pub mod session_log;
pub mod steer;
pub mod storage;
pub mod telemetry;
mod template;
#[cfg(test)]
pub(crate) mod test_support;

pub use models::{
    AgentExecuteResponse, AgentMeta, AgentNode, AgentSecurityConfig, AgentType, ApiKeyConfig,
    ApprovalStatus, AskMode, BinaryRequirement, ChatExecutionStatus, ChatMessage, ChatRole,
    ChatSession, ChatSessionMetadata, ChatSessionSummary, ChatSessionUpdate, ChildRunListQuery,
    CliExecutionConfig, CodexCliExecutionMode, CommandPattern, ContinuationConfig,
    EnvVarRequirement, ExecutionContainerKind, ExecutionContainerRef, ExecutionContainerSummary,
    ExecutionDetails, ExecutionLogField, ExecutionLogQuery, ExecutionLogResponse,
    ExecutionMetricQuery, ExecutionMetricsResponse, ExecutionMode, ExecutionStep,
    ExecutionStepInfo, ExecutionThread, ExecutionTimeline, ExecutionTraceCategory,
    ExecutionTraceEvent, ExecutionTraceQuery, ExecutionTraceSource, ExecutionTraceStats,
    ExecutionTraceTimeRange, GatingCheckResult, GatingRequirements, InstallStatus, InstalledSkill,
    LifecycleTrace, LlmCallTrace, LogRecordTrace, MessageExecution, MessageTrace, MetricDimension,
    MetricSampleTrace, ModelId, ModelMetadataDTO, ModelRoutingConfig, ModelSwitchTrace, OsType,
    PendingApproval, Provider, ProviderHealthQuery, ProviderHealthResponse, ProviderHealthTrace,
    ResourceLimits, RunArtifact, RunArtifactKind, RunKind, RunListQuery, RunSummary,
    SecurityAction, SecurityCheckResult, SecurityMode, SecurityPolicy, Skill, SkillAuthor,
    SkillDependency, SkillGating, SkillManifest, SkillMeta, SkillPermission, SkillPermissions,
    SkillReference, SkillScript, SkillSource, SkillStatus, SkillVersion, SteerMessage, SteerSource,
    StorageMode, Task, TaskControlAction, TaskConversionResult, TaskEvent, TaskEventType,
    TaskMessage, TaskMessageSource, TaskMessageStatus, TaskPatch, TaskProgress, TaskRun,
    TaskRunMetrics, TaskRunStatus, TaskSchedule, TaskSpec, TaskStatus, TerminalSession,
    TerminalStatus, ToolAction, ToolCallInfo, ToolCallPhase, ToolRule, ValidationError,
    ValidationErrorResponse, VersionRequirement, encode_validation_error,
};
pub use steer::SteerRegistry;

use std::sync::Arc;
use storage::Storage;
use tracing::{info, warn};

/// Core application state shared between daemon-backed application modes
///
/// After AgentFlow refactor, this struct focuses on:
/// - Storage access for Agent, Skill, Trigger, and Secrets
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
            let agent_node = models::AgentNode::with_model(models::ModelId::CodexCli);
            let _created = storage.agents.create_agent(
                crate::storage::agent::DEFAULT_ASSISTANT_NAME.to_string(),
                agent_node,
            )?;
            info!(
                "Default agent created: {}",
                crate::storage::agent::DEFAULT_ASSISTANT_NAME
            );
        }
        Ok(())
    }
}
