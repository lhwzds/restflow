//! Two-stage context management: Prune (zero LLM cost) + Compact (LLM cost).
//!
//! **Prune** runs after the ReAct loop exits, middle-truncating old tool results
//! to shrink the checkpoint for future resume.
//!
//! **Compact** runs inside the loop when estimated tokens approach the context
//! window limit, asking the LLM to generate a handoff summary that replaces
//! old messages.
//!
//! Design references:
//! - OpenCode: two-stage prune+compact, summary-as-boundary, protected tools
//! - Codex CLI: middle-truncation (head+tail), memento handoff summary

mod compact;
mod config;
mod constants;
mod prune;
mod token;

pub use compact::{CompactStats, compact, compact_was_effective, should_compact};
pub use config::ContextManagerConfig;
pub use prune::{PruneStats, prune};
pub use token::{TokenEstimator, estimate_tokens, middle_truncate};

#[cfg(test)]
pub(crate) use compact::{find_compact_split, format_conversation_for_summary};
#[cfg(test)]
pub(crate) use constants::ROLE_OVERHEAD_TOKENS;
#[cfg(test)]
pub(crate) use prune::find_protection_boundary;
#[cfg(test)]
pub(crate) use token::estimate_message_tokens;

#[cfg(test)]
mod tests;
