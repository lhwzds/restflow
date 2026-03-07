pub(super) const CHARS_PER_TOKEN: usize = 4;
pub(crate) const ROLE_OVERHEAD_TOKENS: usize = 4;
pub(super) const MIN_PRUNE_SAVINGS_TOKENS: usize = 5_000;
pub(super) const PRUNE_PROTECTED_TURNS: usize = 3;
pub(super) const COMPACT_TRIGGER_RATIO: f64 = 0.90;
pub(super) const SUMMARY_TRUNCATE_CHARS: usize = 4_000;
pub(super) const COMPACT_MIN_REDUCTION: f64 = 0.70;

pub(super) const HANDOFF_PROMPT: &str = include_str!("../../../assets/agents/handoff_prompt.md");
