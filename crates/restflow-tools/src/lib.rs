//! Unified tool and skill system for RestFlow.
//!
//! This crate provides:
//! - Core tool implementations for local agent execution
//! - Security implementations for shell and filesystem operations
//! - Skill-as-Tool system (SkillAsTool, register_skills)
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `restflow-traits` and re-exported here for convenience.

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

// Re-export core tool implementations.
pub use impls::{BashTool, FileTool};

// Re-export edit tools
pub use impls::{EditTool, MultiEditTool};

// Re-export migrated tool implementations
pub use impls::{
    AgentCrudTool, AuthProfileTool, ConfigTool, DeleteMemoryTool, DiagnosticsTool, ListMemoryTool,
    PatchTool, ProcessTool, ReadMemoryTool, ReplyTool, SaveMemoryTool, SecretGetPolicy,
    SecretsTool, SessionTool, SkillTool, SwitchModelTool, TaskTool,
};

// Re-export tool_registry inline migrated tools
pub use impls::{
    ManageOpsTool, MarketplaceTool, SecurityQueryTool, TerminalTool, UnifiedMemorySearchTool,
};

// Re-export search tools
pub use impls::{GlobTool, GrepTool};

// Re-export batch tool
pub use impls::BatchTool;

// Re-export core-migrated tools
pub use impls::{
    BashConfig, FileConfig, ListSubagentsTool, LoadSkillTool, RunSkillTool, SecretsConfig,
    SpawnSubagentBatchTool, SpawnSubagentTool, SpawnTool, ToolRegistryBuilder, WaitSubagentsTool,
    default_registry,
};

// Re-export skill types from restflow-traits
pub use restflow_traits::skill::{SkillContent, SkillInfo, SkillProvider};
pub use skill::loader::register_skills;
pub use skill::tool::SkillAsTool;

// Re-export security implementations
pub use security::bash_security::BashSecurityConfig;

// Re-export HTTP client utilities for consumers that still own runtime HTTP side effects.
pub use http_client::{build_http_client, build_ssrf_safe_client};

#[cfg(test)]
mod tests {
    use super::TaskTool;
    use std::sync::Arc;

    #[test]
    fn crate_root_exports_task_tool_as_canonical_surface() {
        let _: fn(Arc<dyn restflow_traits::store::TaskStore>) -> TaskTool = TaskTool::new;
    }
}
