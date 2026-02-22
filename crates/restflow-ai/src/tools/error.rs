//! Error types for the tools module.
//!
//! Canonical definitions live in `restflow-traits`. This module re-exports them
//! and adds the `Http` variant that depends on `reqwest`.

pub use restflow_traits::error::{Result, ToolError};
