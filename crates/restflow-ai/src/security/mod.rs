//! Security policy module for AI agents.
//!
//! Provides immutable security policies that are injected into agent system prompts
//! to defend against prompt injection, sandbox escape, and credential exposure.

mod policy;
mod gate;

pub use gate::{SecurityDecision, SecurityGate, ToolAction};
pub use policy::{SecurityPolicyConfig, IMMUTABLE_SECURITY_POLICY};
