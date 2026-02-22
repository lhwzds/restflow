//! Unified tool and skill system for RestFlow.
//!
//! This crate provides:
//! - All tool implementations (bash, file, http, email, telegram, discord, slack, and 30+ more)
//! - Security implementations (BashSecurityChecker, network SSRF protection)
//! - Skill-as-Tool system (SkillAsTool, register_skills)
//! - HTTP client utilities
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `restflow-ai` and re-exported here for convenience.

// Re-export modules (backward compatibility shims that delegate to restflow-ai)
pub mod error;
pub mod http_client;
pub mod registry;
pub mod tool;
pub mod toolset;
pub mod wrapper;

// Implementation modules (owned by this crate)
pub mod impls;
pub mod security;
pub mod skill;

// Re-export core types from restflow-ai at crate root
pub use restflow_ai::tools::error::{Result, ToolError};
pub use restflow_ai::tools::traits::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use restflow_ai::tools::registry::ToolRegistry;
pub use restflow_ai::tools::toolset::{Toolset, ToolsetContext};
pub use restflow_ai::tools::wrapper::{
    RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool,
};

// Re-export security types from restflow-ai
pub use restflow_ai::security::{SecurityDecision, SecurityGate, ToolAction};
pub use restflow_ai::security::network::{
    NetworkAllowlist, NetworkEcosystem, resolve_and_validate_url, validate_url,
};

// Store traits are defined in restflow-ai::tools::store_traits.
// Consumers should import them directly from restflow-ai.

// Re-export tool implementations (original 7)
pub use impls::{
    BashTool, DiscordTool, EmailTool, FileTool, HttpTool, SlackTool, TelegramTool,
};
pub use impls::telegram::send_telegram_notification;

// Re-export migrated tool implementations
pub use impls::{
    AgentCrudTool, AuthProfileTool, BackgroundAgentTool, ConfigTool, DiagnosticsTool,
    FilteredToolset, JinaReaderTool, McpServerConfig, MemoryManagementTool,
    MemorySearchMatch, MemorySearchTool, SemanticMemory,
    DeleteMemoryTool, ListMemoryTool, ReadMemoryTool, SaveMemoryTool,
    PythonTool, RunPythonTool, PatchTool, ProcessTool,
    PythonExecutionBackend, PythonExecutionLimits,
    ReplyTool, SaveDeliverableTool, SecretsTool, SessionTool, SkillTool,
    SpawnSubtaskRequest, SpawnSubtaskTool, SwitchModelTool,
    TranscribeConfig, TranscribeTool, VisionTool,
    WebFetchTool, WebSearchTool, WorkspaceNoteTool,
    get_mcp_tools, invalidate_mcp_cache,
};

// Re-export tool_registry inline migrated tools
pub use impls::{
    ManageOpsTool, MarketplaceTool, SecurityQueryTool, SharedSpaceTool,
    TerminalTool, TriggerTool, UnifiedMemorySearchTool,
};

// Re-export core-migrated tools
pub use impls::{
    BashConfig, FileConfig, ListAgentsTool, SpawnAgentTool, SpawnTool,
    ToolRegistryBuilder, UseSkillTool, WaitAgentsTool, default_registry,
};

// Re-export skill types from restflow-ai
pub use restflow_ai::tools::skill_types::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate,
};
pub use skill::tool::SkillAsTool;
pub use skill::loader::register_skills;

// Re-export security implementations
pub use security::bash_security::{BashSecurityChecker, BashSecurityConfig, SecurityCheckResult};

// Re-export http client utilities
pub use http_client::{build_http_client, build_ssrf_safe_client};
