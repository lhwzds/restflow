//! Unified tool and skill system for RestFlow.
//!
//! This crate provides:
//! - Core tool implementations for local agent execution
//! - Security implementations for shell and filesystem operations
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `types` and re-exported here for convenience.

// Implementation modules (owned by this crate)
pub mod impls;

// Re-export core types from types at crate root
pub use types::error::{Result, ToolError};
pub use types::tool::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use types::toolset::ToolRegistry;
pub use types::toolset::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};
pub use types::toolset::{Toolset, ToolsetContext};

// Re-export security types from types
pub use types::network::{
    NetworkAllowlist, NetworkEcosystem, resolve_and_validate_url, validate_url,
};
pub use types::tool::{SecurityDecision, SecurityGate, ToolAction};

// Store traits are defined in types::store.
// Consumers should import them directly from types.

// Re-export core tool implementations.
pub use impls::{BashTool, FileTool};

// Re-export edit tools
pub use impls::{EditTool, MultiEditTool};

// Re-export migrated tool implementations
pub use impls::{
    AgentCrudTool, ConfigTool, PatchTool, ProcessTool, ReplyTool, SecretGetPolicy, SecretsTool,
    SessionTool, SkillTool, SwitchModelTool,
};

// Re-export tool_registry inline migrated tools
pub use impls::ManageOpsTool;

// Re-export search tools
pub use impls::{GlobTool, GrepTool};

// Re-export batch tool
pub use impls::BatchTool;

// Re-export core-migrated tools
pub use impls::{
    BashConfig, FileConfig, ListSubagentsTool, LoadSkillTool, RunSkillTool, SecretsConfig,
    SpawnSubagentBatchTool, SpawnSubagentTool, ToolRegistryBuilder, WaitSubagentsTool,
    default_registry,
};

// Re-export skill types from types
pub use types::skill::{SkillContent, SkillInfo, SkillProvider};

/// Bash command security configuration.
#[derive(Debug, Clone)]
pub struct BashSecurityConfig {
    pub blocked_commands: Vec<String>,
    pub allow_sudo: bool,
}

impl Default for BashSecurityConfig {
    fn default() -> Self {
        Self {
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "mkfs".to_string(),
                "dd if=/dev".to_string(),
                ":(){ :|:& };:".to_string(),
                "chmod -R 777 /".to_string(),
                "chown -R".to_string(),
                "> /dev/sda".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                "init 0".to_string(),
                "halt".to_string(),
            ],
            allow_sudo: false,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_exports_core_tool_surface() {
        let _ = std::mem::size_of::<super::BashTool>();
    }
}
