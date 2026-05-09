//! Unified tool and skill system for RestFlow.
//!
//! This crate provides:
//! - Core tool implementations for local agent execution
//! - Security implementations for shell and filesystem operations
//! - Skill-as-Tool system (SkillAsTool, register_skills)
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `types` and re-exported here for convenience.

pub mod http_client;

// Implementation modules (owned by this crate)
pub mod impls;
pub mod security;
pub mod skill;

// Re-export core types from types at crate root
pub use types::error::{Result, ToolError};
pub use types::registry::ToolRegistry;
pub use types::tool::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use types::toolset::{Toolset, ToolsetContext};
pub use types::wrapper::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};

// Re-export security types from types
pub use types::network::{
    NetworkAllowlist, NetworkEcosystem, resolve_and_validate_url, validate_url,
};
pub use types::security::{SecurityDecision, SecurityGate, ToolAction};

// Store traits are defined in types::store.
// Consumers should import them directly from types.

// Re-export core tool implementations.
pub use impls::{BashTool, FileTool};

// Re-export edit tools
pub use impls::{EditTool, MultiEditTool};

// Re-export migrated tool implementations
pub use impls::{
    AgentCrudTool, AuthProfileTool, ConfigTool, PatchTool, ProcessTool, ReplyTool, SecretGetPolicy,
    SecretsTool, SessionTool, SkillTool, SwitchModelTool, TaskTool,
};

// Re-export tool_registry inline migrated tools
pub use impls::{ManageOpsTool, MarketplaceTool, SecurityQueryTool, TerminalTool};

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

// Re-export skill types from types
pub use skill::loader::register_skills;
pub use skill::tool::SkillAsTool;
pub use types::skill::{SkillContent, SkillInfo, SkillProvider};

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
        let _: fn(Arc<dyn types::store::TaskStore>) -> TaskTool = TaskTool::new;
    }
}
