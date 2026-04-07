//! Unified tool and skill system for RestFlow.
//!
//! This crate provides:
//! - All tool implementations (bash, file, http, email, telegram, discord, slack, and 30+ more)
//! - Security implementations (BashSecurityConfig, network SSRF protection)
//! - Skill-as-Tool system (SkillAsTool, register_skills)
//! - HTTP client utilities
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `restflow-traits` and re-exported here for convenience.

pub mod audio;
pub mod http_client;

// Implementation modules (owned by this crate)
pub mod impls;
pub mod security;
pub mod skill;

// Re-export core types from restflow-traits at crate root
pub use restflow_traits::error::{Result, ToolError};
pub use restflow_traits::registry::ToolRegistry;
pub use restflow_traits::tool::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use restflow_traits::toolset::{Toolset, ToolsetContext};
pub use restflow_traits::wrapper::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};

// Re-export security types from restflow-traits
pub use restflow_traits::network::{
    NetworkAllowlist, NetworkEcosystem, resolve_and_validate_url, validate_url,
};
pub use restflow_traits::security::{SecurityDecision, SecurityGate, ToolAction};

// Store traits are defined in restflow-traits::store.
// Consumers should import them directly from restflow-traits.

// Re-export tool implementations (original 7)
pub use impls::BrowserTool;
pub use impls::telegram::send_telegram_notification;
pub use impls::{BashTool, DiscordTool, EmailTool, FileTool, HttpTool, SlackTool, TelegramTool};

// Re-export edit tools
pub use impls::{EditTool, MultiEditTool};

// Re-export migrated tool implementations
pub use impls::{
    AgentCrudTool, AuthProfileTool, ConfigTool, DeleteMemoryTool, DiagnosticsTool, JinaReaderTool,
    ListMemoryTool, MemoryManagementTool, PatchTool, ProcessTool, PythonExecutionBackend,
    PythonExecutionLimits, PythonTool, ReadMemoryTool, ReplyTool, RunPythonTool,
    SaveDeliverableTool, SaveMemoryTool, SecretGetPolicy, SecretsTool, SessionTool, SkillTool,
    SwitchModelTool, TaskTool, TranscribeConfig, TranscribeTool, VisionTool, WebFetchTool,
    WebSearchTool, WorkItemTool,
};

// Re-export tool_registry inline migrated tools
pub use impls::{
    KvStoreTool, ManageOpsTool, ManageTeamsTool, MarketplaceTool, SecurityQueryTool,
    TerminalTool, TriggerTool, UnifiedMemorySearchTool,
};

// Re-export search tools
pub use impls::{GlobTool, GrepTool, TaskListTool};

// Re-export batch tool
pub use impls::BatchTool;

// Re-export core-migrated tools
pub use impls::{
    BashConfig, FileConfig, ListSubagentsTool, SecretsConfig, SpawnSubagentTool, SpawnTool,
    ToolRegistryBuilder, UseSkillTool, WaitSubagentsTool, default_registry,
};

// Legacy compatibility exports.
pub use impls::BackgroundAgentTool;

// Re-export skill types from restflow-traits
pub use restflow_traits::skill::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate,
};
pub use skill::loader::register_skills;
pub use skill::tool::SkillAsTool;

// Re-export security implementations
pub use security::bash_security::BashSecurityConfig;

// Re-export http client utilities
pub use http_client::{build_http_client, build_ssrf_safe_client};

#[cfg(test)]
mod tests {
    use super::{BackgroundAgentTool, TaskTool};
    use std::sync::Arc;

    #[test]
    fn crate_root_exports_task_tool_as_canonical_surface() {
        let _: fn(Arc<dyn restflow_traits::store::BackgroundAgentStore>) -> TaskTool =
            TaskTool::new;
    }

    #[test]
    fn crate_root_keeps_background_agent_tool_as_legacy_alias() {
        let _: fn(Arc<dyn restflow_traits::store::BackgroundAgentStore>) -> BackgroundAgentTool =
            BackgroundAgentTool::new;
    }
}
