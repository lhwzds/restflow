pub mod auth;
pub(crate) mod boundary;
pub mod channel;
pub mod daemon;
pub mod features;
pub mod hooks;
pub mod loader;
pub mod lsp;
pub mod mcp;
pub mod memory;
pub mod models;
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
pub mod telemetry;
mod template;
#[cfg(test)]
pub(crate) mod test_support;

pub use models::{
    AgentCheckpoint, AgentExecuteResponse, AgentMeta, AgentNode, AgentSecurityConfig, AgentType,
    ApiKeyConfig, ApprovalStatus, AskMode, BinaryRequirement, ChatExecutionStatus, ChatMessage,
    ChatRole, ChatSession, ChatSessionMetadata, ChatSessionSummary, ChatSessionUpdate,
    ChildRunListQuery, CliExecutionConfig, CodexCliExecutionMode, CommandPattern,
    ContinuationConfig, DurabilityMode, EnvVarRequirement, ExecutionContainerKind,
    ExecutionContainerRef, ExecutionContainerSummary, ExecutionDetails, ExecutionLogField,
    ExecutionLogQuery, ExecutionLogResponse, ExecutionMetricQuery, ExecutionMetricsResponse,
    ExecutionMode, ExecutionStep, ExecutionStepInfo, ExecutionThread, ExecutionTimeline,
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery, ExecutionTraceSource,
    ExecutionTraceStats, ExecutionTraceTimeRange, GatingCheckResult, GatingRequirements, Hook,
    HookAction, HookContext, HookEvent, HookFilter, InstallStatus, InstalledSkill, LifecycleTrace,
    LlmCallTrace, LogRecordTrace, MemoryChunk, MemoryConfig, MemoryScope, MemorySearchQuery,
    MemorySearchResult, MemorySession, MemorySource, MemoryStats, MessageExecution, MessageTrace,
    MetricDimension, MetricSampleTrace, ModelId, ModelMetadataDTO, ModelRoutingConfig,
    ModelSwitchTrace, NotificationConfig, OsType, PendingApproval, Provider, ProviderHealthQuery,
    ProviderHealthResponse, ProviderHealthTrace, ResourceLimits, ResumePayload, RunArtifact,
    RunArtifactKind, RunKind, RunListQuery, RunSummary, SearchMode, SecurityAction,
    SecurityCheckResult, SecurityMode, SecurityPolicy, Skill, SkillAuthor, SkillDependency,
    SkillGating, SkillManifest, SkillMeta, SkillPermission, SkillPermissions, SkillReference,
    SkillScript, SkillSource, SkillStatus, SkillVersion, SourceTypeFilter, SteerMessage,
    SteerSource, StorageMode, Task, TaskControlAction, TaskConversionResult, TaskEvent,
    TaskEventType, TaskMessage, TaskMessageSource, TaskMessageStatus, TaskPatch, TaskProgress,
    TaskRun, TaskRunMetrics, TaskRunStatus, TaskSchedule, TaskSpec, TaskStatus, TerminalSession,
    TerminalStatus, ToolAction, ToolCallInfo, ToolCallPhase, ToolRule, UnifiedSearchQuery,
    ValidationError, ValidationErrorResponse, VersionRequirement, WebhookConfig,
    WebhookRateLimiter, WebhookRequest, WebhookResponse, encode_validation_error,
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

        // Sync user filesystem-backed skills into database records.
        if let Ok(user_skills_dir) = paths::user_skills_dir() {
            let report = services::skill_sync::sync_all(&core, &user_skills_dir).await?;
            info!(
                scanned = report.scanned,
                created = report.created,
                updated = report.updated,
                skipped = report.skipped,
                failed = report.failed,
                "User skills synchronized"
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
