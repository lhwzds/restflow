use restflow_traits::{
    DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS, DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS,
    DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
};

use super::constants::{COMPACT_TRIGGER_RATIO, MIN_PRUNE_SAVINGS_TOKENS, PRUNE_PROTECTED_TURNS};

/// Configuration for the two-stage context manager.
#[derive(Debug, Clone)]
pub struct ContextManagerConfig {
    pub context_window: usize,
    pub prune_tool_max: usize,
    pub prune_protected_turns: usize,
    pub min_prune_savings_tokens: usize,
    pub compact_trigger_ratio: f64,
    pub compact_preserve_tokens: usize,
}

impl Default for ContextManagerConfig {
    fn default() -> Self {
        Self {
            context_window: DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS,
            prune_tool_max: DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
            prune_protected_turns: PRUNE_PROTECTED_TURNS,
            min_prune_savings_tokens: MIN_PRUNE_SAVINGS_TOKENS,
            compact_trigger_ratio: COMPACT_TRIGGER_RATIO,
            compact_preserve_tokens: DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS,
        }
    }
}

impl ContextManagerConfig {
    /// Override the context window size.
    pub fn with_context_window(mut self, tokens: usize) -> Self {
        self.context_window = tokens;
        self
    }

    /// Override the pruned tool output size limit.
    pub fn with_prune_tool_max(mut self, max_chars: usize) -> Self {
        self.prune_tool_max = max_chars;
        self
    }

    /// Override the preserved recent token budget for compaction.
    pub fn with_compact_preserve_tokens(mut self, tokens: usize) -> Self {
        self.compact_preserve_tokens = tokens;
        self
    }
}
