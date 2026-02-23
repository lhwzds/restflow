//! Security implementations for tool execution.

// Re-export abstractions from restflow-traits
pub use restflow_traits::security::{SecurityDecision, SecurityGate, ToolAction};
pub use restflow_traits::error::Result;

// Network security types are defined in restflow-traits
pub use restflow_traits::network::{
    NetworkAllowlist, NetworkEcosystem, is_restricted_ip, resolve_and_validate_url, validate_url,
};

/// Bash command security configuration.
pub mod bash_security {
    /// Configuration for bash command security.
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
}
