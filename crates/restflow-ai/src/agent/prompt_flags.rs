//! Prompt composition flags for conditional section inclusion
//!
//! This module provides feature-flag-like control over which sections
//! are included in the agent system prompt.

use serde::{Deserialize, Serialize};

/// Flags controlling which sections are included in the agent system prompt.
///
/// By default, all sections are enabled. Individual sections can be toggled
/// off for specific use cases (e.g., security-sensitive environments, minimal
/// prompts for lightweight agents).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptFlags {
    /// Include base system prompt (identity, role).
    /// Default: true
    #[serde(default = "default_true")]
    pub include_base: bool,

    /// Include available tools section.
    /// Default: true
    #[serde(default = "default_true")]
    pub include_tools: bool,

    /// Include workspace context (file contents from context discovery).
    /// Default: true
    #[serde(default = "default_true")]
    pub include_workspace_context: bool,

    /// Include agent context (skills, memory summary).
    /// Default: true
    #[serde(default = "default_true")]
    pub include_agent_context: bool,

    /// Include security policy section (XPIA, tool restrictions).
    /// Default: true
    #[serde(default = "default_true")]
    pub include_security_policy: bool,
}

impl Default for PromptFlags {
    fn default() -> Self {
        Self {
            include_base: true,
            include_tools: true,
            include_workspace_context: true,
            include_agent_context: true,
            include_security_policy: true,
        }
    }
}

fn default_true() -> bool {
    true
}

impl PromptFlags {
    /// Create a new PromptFlags with all sections enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create PromptFlags with all sections disabled.
    pub fn none() -> Self {
        Self {
            include_base: false,
            include_tools: false,
            include_workspace_context: false,
            include_agent_context: false,
            include_security_policy: false,
        }
    }

    /// Builder: disable base prompt section.
    pub fn without_base(mut self) -> Self {
        self.include_base = false;
        self
    }

    /// Builder: disable tools section.
    pub fn without_tools(mut self) -> Self {
        self.include_tools = false;
        self
    }

    /// Builder: disable workspace context section.
    pub fn without_workspace_context(mut self) -> Self {
        self.include_workspace_context = false;
        self
    }

    /// Builder: disable agent context section.
    pub fn without_agent_context(mut self) -> Self {
        self.include_agent_context = false;
        self
    }

    /// Builder: disable security policy section.
    pub fn without_security_policy(mut self) -> Self {
        self.include_security_policy = false;
        self
    }

    /// Builder: enable only specified sections.
    pub fn only_base() -> Self {
        Self::none().with_base()
    }

    /// Builder: enable base section.
    pub fn with_base(mut self) -> Self {
        self.include_base = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_all_enabled() {
        let flags = PromptFlags::default();
        assert!(flags.include_base);
        assert!(flags.include_tools);
        assert!(flags.include_workspace_context);
        assert!(flags.include_agent_context);
        assert!(flags.include_security_policy);
    }

    #[test]
    fn test_none_all_disabled() {
        let flags = PromptFlags::none();
        assert!(!flags.include_base);
        assert!(!flags.include_tools);
        assert!(!flags.include_workspace_context);
        assert!(!flags.include_agent_context);
        assert!(!flags.include_security_policy);
    }

    #[test]
    fn test_builder_chain() {
        let flags = PromptFlags::new()
            .without_tools()
            .without_workspace_context();

        assert!(flags.include_base);
        assert!(!flags.include_tools);
        assert!(!flags.include_workspace_context);
        assert!(flags.include_agent_context);
        assert!(flags.include_security_policy);
    }

    #[test]
    fn test_only_base() {
        let flags = PromptFlags::only_base();
        assert!(flags.include_base);
        assert!(!flags.include_tools);
        assert!(!flags.include_workspace_context);
        assert!(!flags.include_agent_context);
        assert!(!flags.include_security_policy);
    }

    #[test]
    fn test_serde_roundtrip() {
        let flags = PromptFlags::new().without_tools().without_security_policy();

        let json = serde_json::to_string(&flags).unwrap();
        let parsed: PromptFlags = serde_json::from_str(&json).unwrap();
        assert_eq!(flags, parsed);
    }
}
