use crate::llm::{Message, Role};

use super::config::ContextManagerConfig;
use super::constants::CHARS_PER_TOKEN;
use super::token::middle_truncate;

/// Statistics from a prune operation.
#[derive(Debug, Clone, Default)]
pub struct PruneStats {
    pub messages_truncated: usize,
    pub bytes_removed: usize,
    pub tokens_saved: usize,
    pub applied: bool,
}

/// Find the protection boundary: everything from the last N user turns onward
/// is protected from pruning.
pub(crate) fn find_protection_boundary(messages: &[Message], protected_turns: usize) -> usize {
    if protected_turns == 0 {
        return messages.len();
    }
    let mut count = 0;
    for (i, msg) in messages.iter().enumerate().rev() {
        if msg.role == Role::User {
            count += 1;
            if count >= protected_turns {
                return i;
            }
        }
    }
    // Fewer user turns than protected_turns -> protect everything.
    0
}

/// Prune old tool results by middle-truncation. Two-pass: calculate savings,
/// then apply only if savings exceed the minimum threshold.
///
/// Only Tool messages before the protection boundary are candidates.
/// System, User, and Assistant messages are never pruned.
pub fn prune(messages: &mut [Message], config: &ContextManagerConfig) -> PruneStats {
    let boundary = find_protection_boundary(messages, config.prune_protected_turns);
    if boundary == 0 {
        return PruneStats::default();
    }

    // Pass 1: calculate potential savings.
    // Start from index 1 to skip system prompt (index 0).
    let mut candidates: Vec<usize> = Vec::new();
    let mut total_savings_bytes: usize = 0;

    for (i, msg) in messages.iter().enumerate().take(boundary) {
        if i == 0 {
            continue; // skip system prompt
        }
        if msg.role == Role::Tool && msg.content.len() > config.prune_tool_max {
            let savings = msg.content.len() - config.prune_tool_max;
            total_savings_bytes += savings;
            candidates.push(i);
        }
    }

    let tokens_saved = total_savings_bytes / CHARS_PER_TOKEN;
    if tokens_saved < config.min_prune_savings_tokens {
        return PruneStats {
            applied: false,
            tokens_saved,
            ..Default::default()
        };
    }

    // Pass 2: apply truncation.
    let mut bytes_removed: usize = 0;
    for &idx in &candidates {
        let original_len = messages[idx].content.len();
        messages[idx].content = middle_truncate(&messages[idx].content, config.prune_tool_max);
        bytes_removed += original_len - messages[idx].content.len();
    }

    PruneStats {
        messages_truncated: candidates.len(),
        bytes_removed,
        tokens_saved: bytes_removed / CHARS_PER_TOKEN,
        applied: true,
    }
}
