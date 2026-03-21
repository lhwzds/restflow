pub mod agent;
pub mod agent_execution;
pub mod agent_meta;
pub mod ai_model;
pub mod audit;
pub mod background_agent;
mod catalog;
pub mod channel_session_binding;
pub mod chat_session;
pub mod checkpoint;
pub mod deliverable;
pub mod execution_trace;
pub mod hook;
pub mod memory;
mod provider_policy;
pub mod security;
pub mod shared_space;
pub mod skill;
pub mod skill_folder;
pub mod skill_marketplace;
pub mod skill_meta;
pub mod steer;
pub mod storage_mode;
pub mod terminal_session;
pub mod trigger;
pub mod validation;
pub mod webhook;
pub mod work_item;

pub use agent::{
    AgentNode, ApiKeyConfig, CodexCliExecutionMode, ModelRoutingConfig, SkillPreflightPolicyMode,
};
pub use agent_execution::{AgentExecuteResponse, ExecutionDetails, ExecutionStep, ToolCallInfo};
pub use agent_meta::{AgentMeta, AgentType};
pub use ai_model::{ModelId, ModelMetadataDTO, ModelRef, Provider};
pub use background_agent::{
    BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent, BackgroundAgentEventType,
    BackgroundAgentPatch, BackgroundAgentSchedule, BackgroundAgentSpec, BackgroundAgentStatus,
    BackgroundMessage, BackgroundMessageSource, BackgroundMessageStatus, BackgroundProgress,
    CliExecutionConfig, ContinuationConfig, DurabilityMode, ExecutionMode, MemoryConfig,
    MemoryScope, NotificationConfig, ResourceLimits, TaskEvent, TaskEventType, TaskSchedule,
};
pub use channel_session_binding::ChannelSessionBinding;
pub use checkpoint::{AgentCheckpoint, ResumePayload};
pub use deliverable::{Deliverable, DeliverableType};
pub use hook::{Hook, HookAction, HookContext, HookEvent, HookFilter};
pub use memory::{
    MemoryChunk, MemorySearchQuery, MemorySearchResult, MemorySession, MemorySource, MemoryStats,
    SearchMode, SourceTypeFilter, UnifiedSearchQuery,
};
pub(crate) use provider_policy::{provider_auth_providers, provider_default_model};
pub use steer::{SteerMessage, SteerSource};
pub use webhook::{WebhookConfig, WebhookRateLimiter, WebhookRequest, WebhookResponse};
pub use work_item::{ItemQuery, ItemStatus, WorkItem, WorkItemPatch, WorkItemSpec};

// Export execution trace types (new naming)
pub use execution_trace::{
    ExecutionLogField, ExecutionLogQuery, ExecutionLogResponse, ExecutionMetricQuery,
    ExecutionMetricsResponse, ExecutionTimeline, ExecutionTraceCategory, ExecutionTraceEvent,
    ExecutionTraceQuery, ExecutionTraceSource, ExecutionTraceStats, ExecutionTraceTimeRange,
    LifecycleTrace, LlmCallTrace, LogRecordTrace, MessageTrace, MetricDimension, MetricSampleTrace,
    ModelSwitchTrace, ProviderHealthQuery, ProviderHealthResponse, ProviderHealthTrace,
    ToolCallCompletion, ToolCallPhase, ToolCallTrace,
};

// Re-export audit types for backward compatibility (aliases to execution_trace)
pub use audit::{
    AuditEvent, AuditEventCategory, AuditEventSource, AuditQuery, AuditStats, AuditTimeRange,
    LifecycleAudit, LlmCallAudit, MessageAudit, ModelSwitchAudit, ToolCallAudit,
};

pub use chat_session::{
    ChatExecutionStatus, ChatMediaType, ChatMessage, ChatMessageMedia, ChatMessageTranscript,
    ChatRole, ChatSession, ChatSessionMetadata, ChatSessionSource, ChatSessionSummary,
    ChatSessionUpdate, ExecutionStepInfo, MessageExecution,
};
pub use restflow_storage::Secret;
pub use security::{
    AgentSecurityConfig, ApprovalStatus, AskMode, CommandPattern, PendingApproval, SecurityAction,
    SecurityCheckResult, SecurityMode, SecurityPolicy, ToolAction, ToolRule,
};
pub use shared_space::{SharedEntry, Visibility};
pub use skill::{Skill, SkillStatus};
pub use skill_folder::{SkillGating, SkillReference, SkillScript};
pub use skill_marketplace::{
    BinaryRequirement, EnvVarRequirement, GatingCheckResult, GatingRequirements, InstallStatus,
    InstalledSkill, OsType, SkillAuthor, SkillDependency, SkillManifest, SkillPermission,
    SkillPermissions, SkillSource, SkillVersion, VersionRequirement,
};
pub use skill_meta::SkillMeta;
pub use storage_mode::StorageMode;
pub use terminal_session::{TerminalSession, TerminalStatus};
pub use trigger::{ActiveTrigger, AuthConfig, TriggerConfig};
pub use validation::{ValidationError, ValidationErrorResponse, encode_validation_error};
