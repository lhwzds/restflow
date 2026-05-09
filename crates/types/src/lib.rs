//! Shared RestFlow types, contracts, traits, and model catalog.
//!
//! This crate provides the shared interfaces used across the RestFlow workspace:
//! - Tool trait, ToolError, ToolRegistry, Toolset
//! - SecurityGate, SecurityDecision, ToolAction
//! - NetworkAllowlist, NetworkEcosystem, SSRF validation
//! - SkillProvider and skill data types
//! - store traits (AgentStore, SessionStore, SecretStore, etc.)
//! - Sub-agent data types and lookup traits
//! - Provider and model catalog types

pub mod assessment;
pub mod batch_template;
pub mod boundary;
pub mod cache;
pub mod catalog;
pub mod config_types;
pub mod contracts;
pub mod defaults;
pub mod error;
pub mod filtered;
#[cfg(feature = "http-client")]
pub mod http_client;
pub mod llm;
pub mod model;
mod model_id;
mod model_metadata;
pub mod network;
pub mod orchestrator;
mod provider;
mod provider_meta;
pub mod registry;
pub mod security;
mod selector;
pub mod skill;
pub mod steer;
pub mod store;
pub mod subagent;
pub mod text;
pub mod tool;
pub mod toolset;
pub mod wrapper;

// ── Top-level re-exports ─────────────────────────────────────────────

// Error types
pub use error::{Result as ToolResult, ToolError};

// Assessment types
pub use assessment::{
    AgentOperationAssessor, AssessmentModelRef, OperationAssessment, OperationAssessmentIntent,
    OperationAssessmentIssue, OperationAssessmentStatus, TaskCommandOutcome,
};

// Tool trait and core types
pub use tool::{SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security};

// Registry and toolset
pub use registry::ToolRegistry;
pub use toolset::{Toolset, ToolsetContext};

// Wrappers
pub use wrapper::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};

// Filtered toolset
pub use filtered::{FilteredToolset, ToolPredicate};

// Security
pub use network::{
    NetworkAllowlist, NetworkEcosystem, is_restricted_ip, resolve_and_validate_url, validate_url,
};
pub use security::{SecurityDecision, SecurityGate, ToolAction};

// Skill types
pub use skill::{SkillContent, SkillInfo, SkillProvider, SkillSource};

// Store traits
pub use store::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest, AuthProfileCreateRequest, AuthProfileStore,
    AuthProfileTestRequest, ConfigStore, CredentialInput, MarketplaceStore, OpsProvider,
    ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo, ReplySender, SecretStore,
    SecurityQueryProvider, SessionCreateRequest, SessionListFilter, SessionSearchQuery,
    SessionStore, TaskArtifactListRequest, TaskControlRequest, TaskConvertSessionRequest,
    TaskCreateRequest, TaskDeleteRequest, TaskMessageListRequest, TaskMessageRequest,
    TaskProgressRequest, TaskStore, TaskUpdateRequest, TerminalStore,
};

// Shared orchestration contracts
pub use batch_template::RuntimeTaskPayload;
pub use orchestrator::{AgentOrchestrator, ExecutionMode, ExecutionOutcome, ExecutionPlan};

// Sub-agent types
pub use subagent::{
    ContractChildRunSpawnRequest, ContractRunSpawnRequest, InlineChildRunConfig, InlineRunConfig,
    InlineSubagentConfig, SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion,
    SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
    SubagentEffectiveLimits, SubagentLimitSource, SubagentManager, SubagentResult, SubagentSpawner,
    SubagentState, SubagentStatus,
};

// LLM switching
pub use llm::{ClientKind, LlmProvider, LlmSwitcher, SwapResult};

// Shared model/provider normalization
pub use model::ModelProvider;
pub use model_id::ModelId;
pub use model_metadata::{ModelMetadata, ModelMetadataDTO};
pub use provider::Provider;
pub use provider_meta::{ALL_PROVIDER_META, ProviderMeta, provider_meta};
pub use selector::{
    ProviderSelector, parse_model_reference, parse_provider_selector, resolve_available_model_name,
    split_provider_qualified_model,
};

/// Runtime model specification consumed by the LLM factory.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub provider: LlmProvider,
    pub client_model: String,
    /// Override the provider's default base URL for this specific model.
    pub base_url: Option<String>,
    pub client_kind: ClientKind,
}

impl ModelSpec {
    pub fn new(
        name: impl Into<String>,
        provider: LlmProvider,
        client_model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            provider,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::Http,
        }
    }

    /// Set a custom base URL override for this model.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn codex(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::CodexCli,
        }
    }

    pub fn opencode(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::OpenAI,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::OpenCodeCli,
        }
    }

    pub fn gemini_cli(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::Google,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::GeminiCli,
        }
    }

    pub fn claude_code(name: impl Into<String>, client_model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: LlmProvider::Anthropic,
            client_model: client_model.into(),
            base_url: None,
            client_kind: ClientKind::ClaudeCodeCli,
        }
    }

    pub fn is_codex_cli(&self) -> bool {
        self.client_kind == ClientKind::CodexCli
    }

    pub fn is_opencode_cli(&self) -> bool {
        self.client_kind == ClientKind::OpenCodeCli
    }

    pub fn is_gemini_cli(&self) -> bool {
        self.client_kind == ClientKind::GeminiCli
    }

    pub fn is_claude_code_cli(&self) -> bool {
        self.client_kind == ClientKind::ClaudeCodeCli
    }

    pub fn is_cli(&self) -> bool {
        self.client_kind.is_cli()
    }
}

// Shared steer/runtime control types
pub use steer::{SteerCommand, SteerMessage, SteerSource};

// Shared transport and IPC contracts
pub use contracts::request;
pub use contracts::{
    ApiKeyResponse, ApprovalHandledResponse, ArchiveResponse, CancelResponse, ChatSessionEvent,
    CleanupReportResponse, ClearResponse, DeleteResponse, DeleteWithIdResponse, ErrorKind,
    ErrorPayload, ExecutionScope, ExecutionStats, IdResponse, IpcDaemonStatus, IpcRequest,
    IpcStreamEvent, OkResponse, PromptResponse, ResponseEnvelope, SecretResponse, SteerResponse,
    StreamEnvelope, StreamEventKind, StreamFrame, TASK_STREAM_EVENT, TaskStreamEvent,
    ToolDefinition, ToolExecutionResult,
};

// Shared default constants
pub use defaults::{
    DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS, DEFAULT_AGENT_BASH_TIMEOUT_SECS,
    DEFAULT_AGENT_BROWSER_TIMEOUT_SECS, DEFAULT_AGENT_CACHE_FILE_MAX_BYTES,
    DEFAULT_AGENT_CACHE_FILE_MAX_ENTRIES, DEFAULT_AGENT_CACHE_PERMISSION_TTL_SECS,
    DEFAULT_AGENT_CACHE_SEARCH_MAX_ENTRIES, DEFAULT_AGENT_CACHE_SEARCH_TTL_SECS,
    DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS, DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS,
    DEFAULT_AGENT_LLM_TIMEOUT_SECS, DEFAULT_AGENT_MAX_DURATION_SECS, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_AGENT_MAX_TOOL_CALLS, DEFAULT_AGENT_MAX_TOOL_CONCURRENCY,
    DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH, DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
    DEFAULT_AGENT_PYTHON_TIMEOUT_SECS, DEFAULT_AGENT_TASK_TIMEOUT_SECS,
    DEFAULT_AGENT_TOOL_TIMEOUT_SECS, DEFAULT_API_WEB_SEARCH_RESULTS,
    DEFAULT_CHAT_MAX_SESSION_HISTORY, DEFAULT_GITHUB_CACHE_TTL_SECS,
    DEFAULT_MARKETPLACE_CACHE_TTL_SECS, DEFAULT_MAX_PARALLEL_SUBAGENTS,
    DEFAULT_PROCESS_SESSION_TTL_SECS, DEFAULT_SUBAGENT_MAX_DEPTH, DEFAULT_SUBAGENT_TIMEOUT_SECS,
    DEFAULT_TASK_MAX_TOOL_CALLS, DEFAULT_TASK_MESSAGE_LIST_LIMIT,
    DEFAULT_TASK_PROGRESS_EVENT_LIMIT, DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS,
    DEFAULT_TASK_RUNNER_POLL_INTERVAL_MS, DEFAULT_WORKSPACE_CONTEXT_MAX_FILE_BYTES,
    DEFAULT_WORKSPACE_CONTEXT_MAX_TOTAL_BYTES, MAX_API_WEB_SEARCH_RESULTS,
};

// Cache types
pub use cache::{AgentCache, CachedSearchResult, SearchMatch};

// Shared text helpers
pub use text::floor_char_boundary;
