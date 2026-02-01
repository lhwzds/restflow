//! Security module for command execution control.
//!
//! This module provides security checking and approval management for
//! executing commands through the agent system.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Security System                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  Command ──► SecurityChecker ──► Check Policy               │
//! │                    │                  │                      │
//! │                    │                  ▼                      │
//! │                    │         ┌───────────────┐               │
//! │                    │         │   Blocklist   │ → Block       │
//! │                    │         └───────────────┘               │
//! │                    │                  │                      │
//! │                    │                  ▼                      │
//! │                    │         ┌───────────────┐               │
//! │                    │         │   Allowlist   │ → Allow       │
//! │                    │         └───────────────┘               │
//! │                    │                  │                      │
//! │                    │                  ▼                      │
//! │                    │         ┌───────────────┐               │
//! │                    │         │  Approval Req │ → Request     │
//! │                    │         └───────────────┘               │
//! │                    │                  │                      │
//! │                    │                  ▼                      │
//! │                    │         ┌───────────────┐               │
//! │                    │         │    Default    │               │
//! │                    │         └───────────────┘               │
//! │                    │                                         │
//! │                    ▼                                         │
//! │           ApprovalManager                                    │
//! │             (pending approvals, callbacks)                   │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use restflow_core::security::{SecurityChecker, ApprovalManager};
//! use restflow_core::models::SecurityPolicy;
//!
//! let policy = SecurityPolicy::default();
//! let approval_manager = ApprovalManager::new();
//! let checker = SecurityChecker::new(policy, approval_manager);
//!
//! // Check if a command is allowed
//! let result = checker.check_command("ls -la", "task-1", "agent-1").await?;
//! if result.allowed {
//!     // Execute the command
//! } else if result.requires_approval {
//!     // Wait for user approval
//! }
//! ```

mod approval;
mod checker;
mod config_store;
mod path_resolver;
mod shell_parser;

pub use approval::{ApprovalCallback, ApprovalManager};
pub use checker::SecurityChecker;
pub use config_store::SecurityConfigStore;
