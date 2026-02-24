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

use crate::error::Result;
use crate::llm::{CompletionRequest, LlmClient, Message, Role};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CHARS_PER_TOKEN: usize = 4;
const ROLE_OVERHEAD_TOKENS: usize = 4;
const DEFAULT_PRUNE_TOOL_MAX: usize = 2048;
const MIN_PRUNE_SAVINGS_TOKENS: usize = 5_000;
const PRUNE_PROTECTED_TURNS: usize = 3;
const COMPACT_TRIGGER_RATIO: f64 = 0.90;
const COMPACT_PRESERVE_TOKENS: usize = 20_000;
const SUMMARY_TRUNCATE_CHARS: usize = 4000;
/// Minimum token reduction ratio for compact to be considered effective.
/// If compact doesn't reduce tokens by at least this factor, we enter cooldown.
const COMPACT_MIN_REDUCTION: f64 = 0.70;

const HANDOFF_PROMPT: &str = include_str!("../../assets/agents/handoff_prompt.md");

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

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
            context_window: 128_000,
            prune_tool_max: DEFAULT_PRUNE_TOOL_MAX,
            prune_protected_turns: PRUNE_PROTECTED_TURNS,
            min_prune_savings_tokens: MIN_PRUNE_SAVINGS_TOKENS,
            compact_trigger_ratio: COMPACT_TRIGGER_RATIO,
            compact_preserve_tokens: COMPACT_PRESERVE_TOKENS,
        }
    }
}

impl ContextManagerConfig {
    /// Override the context window size.
    pub fn with_context_window(mut self, tokens: usize) -> Self {
        self.context_window = tokens;
        self
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Statistics from a prune operation.
#[derive(Debug, Clone, Default)]
pub struct PruneStats {
    pub messages_truncated: usize,
    pub bytes_removed: usize,
    pub tokens_saved: usize,
    pub applied: bool,
}

/// Statistics from a compact operation.
#[derive(Debug, Clone, Default)]
pub struct CompactStats {
    pub messages_replaced: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub summary_length: usize,
}

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate tokens for a single message (bytes / CHARS_PER_TOKEN + role overhead).
fn estimate_message_tokens(msg: &Message) -> usize {
    let mut bytes = msg.content.len();
    if let Some(calls) = &msg.tool_calls {
        for call in calls {
            bytes += call.id.len() + call.name.len();
            bytes += call.arguments.to_string().len();
        }
    }
    if let Some(id) = &msg.tool_call_id {
        bytes += id.len();
    }
    bytes / CHARS_PER_TOKEN + ROLE_OVERHEAD_TOKENS
}

/// Estimate total tokens for a message list.
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

// ---------------------------------------------------------------------------
// Token estimator with calibration
// ---------------------------------------------------------------------------

/// Exponential-moving-average calibrated token estimator.
///
/// Tracks a rolling `calibration_factor` (ratio of actual to heuristic tokens)
/// and applies it to future estimates. Also provides a compaction cooldown
/// to prevent compaction loops.
#[derive(Debug, Clone)]
pub struct TokenEstimator {
    calibration_factor: f64,
    samples: usize,
    /// Iterations remaining before compact is allowed again.
    compact_cooldown: usize,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            calibration_factor: 1.0,
            samples: 0,
            compact_cooldown: 0,
        }
    }
}

impl TokenEstimator {
    /// Calibrate using the actual prompt_tokens returned by the API.
    pub fn calibrate(&mut self, estimated: usize, actual_prompt_tokens: u32) {
        if estimated == 0 || actual_prompt_tokens == 0 {
            return;
        }
        let ratio = actual_prompt_tokens as f64 / estimated as f64;
        let alpha = if self.samples < 5 { 0.5 } else { 0.2 };
        self.calibration_factor = self.calibration_factor * (1.0 - alpha) + ratio * alpha;
        self.samples += 1;
    }

    /// Return a calibrated token estimate.
    pub fn estimate(&self, messages: &[Message]) -> usize {
        let raw = estimate_tokens(messages);
        (raw as f64 * self.calibration_factor).ceil() as usize
    }

    /// Check if compact is allowed (not in cooldown).
    pub fn compact_allowed(&self) -> bool {
        self.compact_cooldown == 0
    }

    /// Start a cooldown period after an ineffective compaction.
    pub fn start_compact_cooldown(&mut self, iterations: usize) {
        self.compact_cooldown = iterations;
    }

    /// Tick one iteration of cooldown (call once per loop iteration).
    pub fn tick_cooldown(&mut self) {
        self.compact_cooldown = self.compact_cooldown.saturating_sub(1);
    }
}

// ---------------------------------------------------------------------------
// Middle-truncation (Codex-style)
// ---------------------------------------------------------------------------

/// Keep head + tail of a string, inserting a truncation marker in the middle.
/// Uses `floor_char_boundary` logic for UTF-8 safety.
pub fn middle_truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    let marker = format!(
        "\n... [{} chars truncated] ...\n",
        s.len().saturating_sub(max_len)
    );

    if max_len <= marker.len() {
        // Cannot fit anything besides the marker; just return a truncated prefix.
        let end = floor_char_boundary(s, max_len);
        return s[..end].to_string();
    }

    let available = max_len - marker.len();
    let head_len = available / 2;
    let tail_len = available - head_len;

    let head_end = floor_char_boundary(s, head_len);
    let tail_start = ceil_char_boundary(s, s.len().saturating_sub(tail_len));

    format!("{}{}{}", &s[..head_end], marker, &s[tail_start..])
}

/// Find the largest byte index <= `pos` that is a char boundary.
fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut i = pos;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Find the smallest byte index >= `pos` that is a char boundary.
fn ceil_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut i = pos;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

// ---------------------------------------------------------------------------
// Stage 1: Prune (zero LLM cost)
// ---------------------------------------------------------------------------

/// Find the protection boundary: everything from the last N user turns onward
/// is protected from pruning.
fn find_protection_boundary(messages: &[Message], protected_turns: usize) -> usize {
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
    // Fewer user turns than protected_turns → protect everything.
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

// ---------------------------------------------------------------------------
// Stage 2: Compact (LLM cost)
// ---------------------------------------------------------------------------

/// Check whether compaction should be triggered.
pub fn should_compact(estimated_tokens: usize, config: &ContextManagerConfig) -> bool {
    if config.context_window == 0 {
        return false;
    }
    let threshold = (config.context_window as f64 * config.compact_trigger_ratio) as usize;
    estimated_tokens > threshold
}

/// Format conversation transcript for the summarization LLM call.
fn format_conversation_for_summary(messages: &[Message]) -> String {
    let mut out = String::new();
    for msg in messages {
        let role_label = match msg.role {
            Role::System => "SYSTEM",
            Role::User => "USER",
            Role::Assistant => "ASSISTANT",
            Role::Tool => "TOOL",
        };

        let content = if msg.content.len() > SUMMARY_TRUNCATE_CHARS {
            middle_truncate(&msg.content, SUMMARY_TRUNCATE_CHARS)
        } else {
            msg.content.clone()
        };

        out.push_str(&format!("[{}] {}\n\n", role_label, content));

        if let Some(calls) = &msg.tool_calls {
            for call in calls {
                let args_str = call.arguments.to_string();
                let args_display = if args_str.len() > 200 {
                    middle_truncate(&args_str, 200)
                } else {
                    args_str
                };
                out.push_str(&format!("  → tool_call: {}({})\n", call.name, args_display));
            }
        }
    }
    out
}

/// Find split point: preserve recent ~compact_preserve_tokens of messages,
/// aligned to a safe message boundary (never split between an assistant with
/// tool_calls and its corresponding tool results).
fn find_compact_split(messages: &[Message], preserve_tokens: usize) -> usize {
    if messages.is_empty() {
        return 0;
    }

    // Accumulate tokens from the end until we reach preserve_tokens.
    let mut accumulated = 0;
    let mut split = messages.len();

    for i in (0..messages.len()).rev() {
        accumulated += estimate_message_tokens(&messages[i]);
        if accumulated >= preserve_tokens {
            split = i;
            break;
        }
    }

    // Never remove the system prompt (index 0).
    if split <= 1 {
        return 1;
    }

    // Align to safe boundary: if split lands on a Tool message, walk forward
    // past all consecutive Tool messages to avoid orphaning them from their
    // assistant+tool_calls parent.
    while split < messages.len() && messages[split].role == Role::Tool {
        split += 1;
    }

    // Also check: if messages[split-1] is an Assistant with tool_calls,
    // we must include those tool results too — walk forward.
    if split > 0
        && let Some(calls) = &messages[split - 1].tool_calls
        && !calls.is_empty()
    {
        while split < messages.len() && messages[split].role == Role::Tool {
            split += 1;
        }
    }

    split
}

/// Generate a handoff summary and replace old messages.
///
/// Returns `CompactStats` with `messages_replaced == 0` if there's nothing to
/// compact, or if the LLM returns an empty summary (safety: don't replace
/// real history with nothing).
pub async fn compact(
    messages: &mut Vec<Message>,
    config: &ContextManagerConfig,
    llm: &dyn LlmClient,
) -> Result<CompactStats> {
    let tokens_before = estimate_tokens(messages);
    let split = find_compact_split(messages, config.compact_preserve_tokens);

    // Nothing to compact if split is at 1 (only system prompt) or beyond end.
    if split <= 1 || split >= messages.len() {
        return Ok(CompactStats {
            messages_replaced: 0,
            tokens_before,
            tokens_after: tokens_before,
            summary_length: 0,
        });
    }

    // Extract old messages for summarization (skip system prompt at [0]).
    let old_messages = &messages[1..split];
    let transcript = format_conversation_for_summary(old_messages);

    // Ask LLM for handoff summary.
    let summary_request = CompletionRequest::new(vec![
        Message::system(HANDOFF_PROMPT),
        Message::user(transcript),
    ]);

    let response = llm.complete(summary_request).await?;
    let summary = response.content.unwrap_or_default();

    // Safety: don't replace real messages with an empty summary.
    if summary.trim().is_empty() {
        tracing::warn!("LLM returned empty summary, skipping compaction");
        return Ok(CompactStats {
            messages_replaced: 0,
            tokens_before,
            tokens_after: tokens_before,
            summary_length: 0,
        });
    }

    // Rebuild messages: system + summary + preserved tail.
    let system_msg = messages[0].clone();
    let preserved = messages[split..].to_vec();

    let summary_msg = Message::user(format!("[Session Summary]\n\n{}", summary));
    let summary_length = summary.len();

    messages.clear();
    messages.push(system_msg);
    messages.push(summary_msg);
    messages.extend(preserved);

    let tokens_after = estimate_tokens(messages);

    Ok(CompactStats {
        messages_replaced: split - 1, // excluding system prompt
        tokens_before,
        tokens_after,
        summary_length,
    })
}

/// Check whether compaction was effective. If the reduction ratio is too small,
/// the caller should activate a cooldown to prevent compaction loops.
pub fn compact_was_effective(stats: &CompactStats) -> bool {
    if stats.tokens_before == 0 || stats.messages_replaced == 0 {
        return false;
    }
    let ratio = stats.tokens_after as f64 / stats.tokens_before as f64;
    ratio < COMPACT_MIN_REDUCTION
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{MockLlmClient, MockStep, ToolCall};
    use serde_json::json;

    // ======================================================================
    // middle_truncate
    // ======================================================================

    #[test]
    fn middle_truncate_short_string_unchanged() {
        let s = "hello world";
        assert_eq!(middle_truncate(s, 100), s);
    }

    #[test]
    fn middle_truncate_exact_length_unchanged() {
        let s = "hello";
        assert_eq!(middle_truncate(s, 5), s);
    }

    #[test]
    fn middle_truncate_empty_string() {
        assert_eq!(middle_truncate("", 10), "");
    }

    #[test]
    fn middle_truncate_max_len_zero() {
        let s = "hello";
        let result = middle_truncate(s, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn middle_truncate_long_string() {
        let s = "a".repeat(1000);
        let result = middle_truncate(&s, 200);
        assert!(result.len() <= 200);
        assert!(result.contains("chars truncated"));
        assert!(result.starts_with('a'));
        assert!(result.ends_with('a'));
    }

    #[test]
    fn middle_truncate_preserves_head_and_tail_content() {
        // Build a string with distinct head and tail characters.
        let s = format!("{}{}", "H".repeat(500), "T".repeat(500));
        let result = middle_truncate(&s, 200);
        assert!(result.starts_with('H'));
        assert!(result.ends_with('T'));
        assert!(result.contains("chars truncated"));
    }

    #[test]
    fn middle_truncate_result_never_exceeds_max_len() {
        // Test with various sizes to verify invariant.
        for max_len in [50, 100, 200, 500, 1000] {
            let s = "x".repeat(5000);
            let result = middle_truncate(&s, max_len);
            assert!(
                result.len() <= max_len,
                "max_len={max_len}, result.len()={}",
                result.len()
            );
        }
    }

    #[test]
    fn middle_truncate_utf8_safety_chinese() {
        let s = "你好世界".repeat(100); // 4 chars * 3 bytes each * 100 = 1200 bytes
        let result = middle_truncate(&s, 200);
        assert!(result.len() <= 200);
        // Must be valid UTF-8 (would panic on char iteration if not).
        let _ = result.chars().count();
    }

    #[test]
    fn middle_truncate_utf8_safety_emoji() {
        let s = "😀🎉🚀".repeat(50); // 4 bytes per emoji * 3 * 50 = 600 bytes
        let result = middle_truncate(&s, 100);
        let _ = result.chars().count();
    }

    #[test]
    fn middle_truncate_utf8_mixed_content() {
        // Mix of ASCII, CJK, and emoji
        let s = "Hello你好😀World世界🎉".repeat(30);
        let result = middle_truncate(&s, 150);
        assert!(result.len() <= 150);
        let _ = result.chars().count();
    }

    #[test]
    fn middle_truncate_max_len_smaller_than_marker() {
        let s = "a".repeat(100);
        let result = middle_truncate(&s, 5);
        assert_eq!(result.len(), 5);
        assert_eq!(result, "aaaaa");
    }

    #[test]
    fn middle_truncate_marker_shows_correct_count() {
        let s = "a".repeat(1000);
        let result = middle_truncate(&s, 200);
        // Truncated chars = 1000 - 200 = 800
        assert!(result.contains("800 chars truncated"));
    }

    // ======================================================================
    // estimate_tokens
    // ======================================================================

    #[test]
    fn estimate_tokens_basic_message() {
        let msg = Message::user("hello world"); // 11 bytes
        let tokens = estimate_message_tokens(&msg);
        // 11/4 + 4 = 6
        assert_eq!(tokens, 6);
    }

    #[test]
    fn estimate_tokens_empty_content() {
        let msg = Message::user("");
        let tokens = estimate_message_tokens(&msg);
        // 0/4 + 4 = 4 (just role overhead)
        assert_eq!(tokens, ROLE_OVERHEAD_TOKENS);
    }

    #[test]
    fn estimate_tokens_with_tool_calls() {
        let msg = Message::assistant_with_tool_calls(
            Some("thinking".to_string()),
            vec![ToolCall {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: json!({"command": "ls"}),
            }],
        );
        let tokens = estimate_message_tokens(&msg);
        // "thinking" (8) + "call_1" (6) + "bash" (4) + json (~16) = ~34 bytes / 4 + 4 ≈ 12
        assert!(tokens > ROLE_OVERHEAD_TOKENS);
    }

    #[test]
    fn estimate_tokens_tool_result_with_id() {
        let msg = Message::tool_result("call_abc123", "result content here");
        let tokens = estimate_message_tokens(&msg);
        // content (19) + tool_call_id (11) = 30 bytes / 4 + 4 = 11
        assert_eq!(tokens, 11);
    }

    #[test]
    fn estimate_tokens_empty_list() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn estimate_tokens_multiple_messages() {
        let msgs = vec![
            Message::system("You are helpful."),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];
        let total = estimate_tokens(&msgs);
        assert!(total > 3 * ROLE_OVERHEAD_TOKENS); // more than just overhead
    }

    #[test]
    fn estimate_tokens_large_message() {
        let content = "x".repeat(40_000);
        let msg = Message::user(&content);
        let tokens = estimate_message_tokens(&msg);
        // 40000/4 + 4 = 10004
        assert_eq!(tokens, 10_004);
    }

    // ======================================================================
    // TokenEstimator
    // ======================================================================

    #[test]
    fn token_estimator_default_factor() {
        let est = TokenEstimator::default();
        assert!((est.calibration_factor - 1.0).abs() < f64::EPSILON);
        assert_eq!(est.samples, 0);
        assert!(est.compact_allowed());
    }

    #[test]
    fn token_estimator_calibrate_adjusts_factor() {
        let mut est = TokenEstimator::default();
        est.calibrate(100, 200);
        // After first sample (alpha=0.5): 1.0 * 0.5 + 2.0 * 0.5 = 1.5
        assert!((est.calibration_factor - 1.5).abs() < 0.01);
        assert_eq!(est.samples, 1);
    }

    #[test]
    fn token_estimator_ema_converges() {
        let mut est = TokenEstimator::default();
        for _ in 0..20 {
            est.calibrate(100, 150);
        }
        assert!((est.calibration_factor - 1.5).abs() < 0.05);
    }

    #[test]
    fn token_estimator_ema_switches_alpha_after_5_samples() {
        let mut est = TokenEstimator::default();
        // First 5 samples use alpha=0.5 (fast convergence)
        for _ in 0..5 {
            est.calibrate(100, 200);
        }
        let factor_after_5 = est.calibration_factor;
        // After 5 samples, alpha switches to 0.2 (slower)
        est.calibrate(100, 100); // sudden change to ratio=1.0
        let factor_after_6 = est.calibration_factor;
        // With alpha=0.2, change should be small
        let delta = (factor_after_5 - factor_after_6).abs();
        assert!(delta < 0.5, "alpha=0.2 should cause small adjustment");
    }

    #[test]
    fn token_estimator_zero_values_ignored() {
        let mut est = TokenEstimator::default();
        est.calibrate(0, 100);
        assert!((est.calibration_factor - 1.0).abs() < f64::EPSILON);
        est.calibrate(100, 0);
        assert!((est.calibration_factor - 1.0).abs() < f64::EPSILON);
        assert_eq!(est.samples, 0);
    }

    #[test]
    fn token_estimator_estimate_applies_factor() {
        let mut est = TokenEstimator::default();
        est.calibrate(100, 200); // factor → 1.5

        let msgs = vec![Message::user("hello world")]; // raw ≈ 6 tokens
        let raw = estimate_tokens(&msgs);
        let calibrated = est.estimate(&msgs);
        assert!(calibrated > raw);
        assert_eq!(
            calibrated,
            (raw as f64 * est.calibration_factor).ceil() as usize
        );
    }

    #[test]
    fn token_estimator_cooldown() {
        let mut est = TokenEstimator::default();
        assert!(est.compact_allowed());

        est.start_compact_cooldown(3);
        assert!(!est.compact_allowed());

        est.tick_cooldown();
        assert!(!est.compact_allowed()); // 2 remaining

        est.tick_cooldown();
        assert!(!est.compact_allowed()); // 1 remaining

        est.tick_cooldown();
        assert!(est.compact_allowed()); // 0 remaining

        // Extra ticks don't underflow
        est.tick_cooldown();
        assert!(est.compact_allowed());
    }

    // ======================================================================
    // find_protection_boundary
    // ======================================================================

    #[test]
    fn protection_boundary_normal() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let boundary = find_protection_boundary(&msgs, 2);
        assert_eq!(boundary, 3); // index of "u2"
    }

    #[test]
    fn protection_boundary_single_user_turn() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
        ];
        let boundary = find_protection_boundary(&msgs, 1);
        assert_eq!(boundary, 1); // index of "u1"
    }

    #[test]
    fn protection_boundary_fewer_user_turns() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
        ];
        let boundary = find_protection_boundary(&msgs, 3);
        assert_eq!(boundary, 0);
    }

    #[test]
    fn protection_boundary_no_user_messages() {
        let msgs = vec![Message::system("sys"), Message::assistant("a1")];
        let boundary = find_protection_boundary(&msgs, 2);
        assert_eq!(boundary, 0);
    }

    #[test]
    fn protection_boundary_zero_protected() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
        ];
        let boundary = find_protection_boundary(&msgs, 0);
        assert_eq!(boundary, msgs.len());
    }

    #[test]
    fn protection_boundary_empty_messages() {
        let boundary = find_protection_boundary(&[], 2);
        assert_eq!(boundary, 0);
    }

    #[test]
    fn protection_boundary_interleaved_tool_messages() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::tool_result("c1", "r1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::tool_result("c2", "r2"),
            Message::user("u3"),
        ];
        let boundary = find_protection_boundary(&msgs, 2);
        assert_eq!(boundary, 4); // index of "u2"
    }

    // ======================================================================
    // prune
    // ======================================================================

    #[test]
    fn prune_truncates_old_tool_results() {
        let big_content = "x".repeat(20_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::tool_result("call_1", &big_content),
            Message::tool_result("call_2", &big_content),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
            Message::assistant("a3"),
            Message::user("u4"),
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 2,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        assert!(stats.applied);
        assert_eq!(stats.messages_truncated, 2);
        assert!(msgs[2].content.len() <= 2048);
        assert!(msgs[3].content.len() <= 2048);
        assert!(stats.bytes_removed > 0);
        assert!(stats.tokens_saved > 0);
    }

    #[test]
    fn prune_protects_recent_messages() {
        let big_content = "x".repeat(20_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::tool_result("call_1", &big_content), // within protection
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 2,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        // boundary = index 1 (u1), only messages[0..1] scanned = just system prompt
        assert!(!stats.applied);
        assert_eq!(msgs[4].content.len(), 20_000); // unchanged
    }

    #[test]
    fn prune_savings_below_threshold_not_applied() {
        let small_content = "x".repeat(3000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::tool_result("call_1", &small_content),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 5000,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        assert!(!stats.applied);
        assert_eq!(msgs[2].content.len(), 3000); // unchanged
    }

    #[test]
    fn prune_never_modifies_system_message() {
        let big_system = "S".repeat(20_000);
        let big_tool = "T".repeat(20_000);
        let mut msgs = vec![
            Message::system(&big_system),
            Message::tool_result("c1", &big_tool),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        prune(&mut msgs, &config);
        // System message must be unchanged even though it's large.
        assert_eq!(msgs[0].content.len(), 20_000);
    }

    #[test]
    fn prune_skips_non_tool_messages() {
        let big_content = "x".repeat(20_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user(&big_content),              // big but not Tool
            Message::assistant(&big_content),         // big but not Tool
            Message::tool_result("c1", &big_content), // this should be pruned
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
            Message::assistant("a3"),
            Message::user("u4"),
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 2,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        assert!(stats.applied);
        assert_eq!(stats.messages_truncated, 1); // only the Tool message
        assert_eq!(msgs[1].content.len(), 20_000); // User unchanged
        assert_eq!(msgs[2].content.len(), 20_000); // Assistant unchanged
        assert!(msgs[3].content.len() <= 2048); // Tool truncated
    }

    #[test]
    fn prune_already_small_tool_results_untouched() {
        let small_content = "small result";
        let big_content = "x".repeat(20_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::tool_result("c1", small_content),
            Message::tool_result("c2", &big_content),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        assert!(stats.applied);
        assert_eq!(stats.messages_truncated, 1); // only the big one
        assert_eq!(msgs[1].content, "small result"); // unchanged
    }

    #[test]
    fn prune_idempotent() {
        let big_content = "x".repeat(20_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::tool_result("c1", &big_content),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let config = ContextManagerConfig {
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats1 = prune(&mut msgs, &config);
        assert!(stats1.applied);
        let len_after_first = msgs[1].content.len();

        // Second prune should be a no-op (already truncated).
        let stats2 = prune(&mut msgs, &config);
        assert!(!stats2.applied);
        assert_eq!(msgs[1].content.len(), len_after_first);
    }

    #[test]
    fn prune_empty_messages() {
        let mut msgs: Vec<Message> = vec![];
        let config = ContextManagerConfig::default();
        let stats = prune(&mut msgs, &config);
        assert!(!stats.applied);
    }

    #[test]
    fn prune_tokens_saved_is_accurate() {
        let big_content = "x".repeat(10_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::tool_result("c1", &big_content),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let tokens_before = estimate_tokens(&msgs);
        let config = ContextManagerConfig {
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        let tokens_after = estimate_tokens(&msgs);
        assert!(stats.applied);
        // Actual token reduction should match stats.
        let actual_reduction = tokens_before - tokens_after;
        assert_eq!(actual_reduction, stats.tokens_saved);
    }

    // ======================================================================
    // should_compact
    // ======================================================================

    #[test]
    fn should_compact_below_threshold() {
        let config = ContextManagerConfig {
            context_window: 128_000,
            compact_trigger_ratio: 0.90,
            ..Default::default()
        };
        assert!(!should_compact(100_000, &config));
    }

    #[test]
    fn should_compact_above_threshold() {
        let config = ContextManagerConfig {
            context_window: 128_000,
            compact_trigger_ratio: 0.90,
            ..Default::default()
        };
        assert!(should_compact(120_000, &config));
    }

    #[test]
    fn should_compact_exactly_at_threshold() {
        let config = ContextManagerConfig {
            context_window: 100_000,
            compact_trigger_ratio: 0.90,
            ..Default::default()
        };
        // 90% of 100k = 90000. 90000 is NOT > 90000.
        assert!(!should_compact(90_000, &config));
        assert!(should_compact(90_001, &config));
    }

    #[test]
    fn should_compact_zero_context_window() {
        let config = ContextManagerConfig {
            context_window: 0,
            compact_trigger_ratio: 0.90,
            ..Default::default()
        };
        assert!(!should_compact(100_000, &config));
    }

    #[test]
    fn should_compact_zero_tokens() {
        let config = ContextManagerConfig::default();
        assert!(!should_compact(0, &config));
    }

    // ======================================================================
    // find_compact_split
    // ======================================================================

    #[test]
    fn find_compact_split_normal() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
        ];
        let split = find_compact_split(&msgs, 10);
        assert!(split >= 1);
        assert!(split <= msgs.len());
    }

    #[test]
    fn find_compact_split_preserves_tool_call_pairs() {
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant_with_tool_calls(
                Some("thinking".to_string()),
                vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "bash".to_string(),
                    arguments: json!({"command": "ls"}),
                }],
            ),
            Message::tool_result("call_1", "file1.txt\nfile2.txt"),
            Message::user("u2"),
            Message::assistant("done"),
        ];
        let split = find_compact_split(&msgs, 20);
        // Split must not land at index 3 (tool result orphaned from assistant).
        assert!(
            split <= 2 || split >= 4,
            "split={split} would orphan tool result at index 3"
        );
    }

    #[test]
    fn find_compact_split_skips_consecutive_tool_results() {
        // Simulate an assistant calling 3 tools in parallel.
        let msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant_with_tool_calls(
                None,
                vec![
                    ToolCall {
                        id: "c1".to_string(),
                        name: "bash".to_string(),
                        arguments: json!({}),
                    },
                    ToolCall {
                        id: "c2".to_string(),
                        name: "file".to_string(),
                        arguments: json!({}),
                    },
                    ToolCall {
                        id: "c3".to_string(),
                        name: "http".to_string(),
                        arguments: json!({}),
                    },
                ],
            ),
            Message::tool_result("c1", "r1"),
            Message::tool_result("c2", "r2"),
            Message::tool_result("c3", "r3"),
            Message::user("u2"),
            Message::assistant("done"),
        ];
        let split = find_compact_split(&msgs, 15);
        // Must not land on any of the tool results (index 3, 4, 5).
        assert!(
            split <= 2 || split >= 6,
            "split={split} would orphan tool results"
        );
    }

    #[test]
    fn find_compact_split_empty_messages() {
        assert_eq!(find_compact_split(&[], 1000), 0);
    }

    #[test]
    fn find_compact_split_preserves_system_prompt() {
        let msgs = vec![Message::system("sys"), Message::user("u1")];
        let split = find_compact_split(&msgs, 1_000_000);
        assert_eq!(split, msgs.len());

        let msgs2 = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
        ];
        let split2 = find_compact_split(&msgs2, 5);
        assert!(split2 >= 1, "split should never remove the system prompt");
    }

    #[test]
    fn find_compact_split_single_message_after_system() {
        let msgs = vec![Message::system("sys"), Message::user("u1")];
        let split = find_compact_split(&msgs, 5);
        // Only 2 messages, split ≤ 1 → returns 1
        assert_eq!(split, 1);
    }

    // ======================================================================
    // format_conversation_for_summary
    // ======================================================================

    #[test]
    fn format_conversation_basic() {
        let msgs = vec![Message::user("Hello"), Message::assistant("Hi there!")];
        let formatted = format_conversation_for_summary(&msgs);
        assert!(formatted.contains("[USER] Hello"));
        assert!(formatted.contains("[ASSISTANT] Hi there!"));
    }

    #[test]
    fn format_conversation_truncates_long_messages() {
        let long_msg = "x".repeat(10_000);
        let msgs = vec![Message::user(&long_msg)];
        let formatted = format_conversation_for_summary(&msgs);
        assert!(formatted.contains("chars truncated"));
    }

    #[test]
    fn format_conversation_short_messages_not_truncated() {
        let msgs = vec![Message::user("short message")];
        let formatted = format_conversation_for_summary(&msgs);
        assert!(!formatted.contains("chars truncated"));
        assert!(formatted.contains("short message"));
    }

    #[test]
    fn format_conversation_includes_tool_calls() {
        let msgs = vec![Message::assistant_with_tool_calls(
            Some("let me check".to_string()),
            vec![ToolCall {
                id: "c1".to_string(),
                name: "bash".to_string(),
                arguments: json!({"command": "ls -la"}),
            }],
        )];
        let formatted = format_conversation_for_summary(&msgs);
        assert!(formatted.contains("tool_call: bash("));
    }

    #[test]
    fn format_conversation_long_tool_args_truncated() {
        let long_args = json!({"data": "x".repeat(500)});
        let msgs = vec![Message::assistant_with_tool_calls(
            None,
            vec![ToolCall {
                id: "c1".to_string(),
                name: "http".to_string(),
                arguments: long_args,
            }],
        )];
        let formatted = format_conversation_for_summary(&msgs);
        assert!(formatted.contains("chars truncated"));
    }

    #[test]
    fn format_conversation_includes_all_roles() {
        let msgs = vec![
            Message::system("system instructions"),
            Message::user("user input"),
            Message::assistant("assistant reply"),
            Message::tool_result("c1", "tool output"),
        ];
        let formatted = format_conversation_for_summary(&msgs);
        assert!(formatted.contains("[SYSTEM]"));
        assert!(formatted.contains("[USER]"));
        assert!(formatted.contains("[ASSISTANT]"));
        assert!(formatted.contains("[TOOL]"));
    }

    #[test]
    fn format_conversation_empty() {
        let formatted = format_conversation_for_summary(&[]);
        assert!(formatted.is_empty());
    }

    // ======================================================================
    // compact (async tests with MockLlmClient)
    // ======================================================================

    #[tokio::test]
    async fn compact_replaces_old_messages_with_summary() {
        let mock = MockLlmClient::from_steps(
            "mock",
            vec![MockStep::text(
                "Goal: fix bug. Done: edited main.rs. Remaining: tests.",
            )],
        );
        let mut msgs = vec![
            Message::system("You are helpful."),
            Message::user("Fix the bug in main.rs"),
            Message::assistant("Looking at the file..."),
            Message::tool_result("c1", "fn main() { ... }"),
            Message::user("Good, now add tests"),
            Message::assistant("I'll add tests."),
        ];
        let config = ContextManagerConfig {
            compact_preserve_tokens: 10, // preserve very little
            ..Default::default()
        };

        let stats = compact(&mut msgs, &config, &mock).await.unwrap();

        assert!(stats.messages_replaced > 0);
        assert!(stats.tokens_after < stats.tokens_before);
        assert!(stats.summary_length > 0);
        // First message should still be the system prompt.
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content, "You are helpful.");
        // Second message should be the summary.
        assert_eq!(msgs[1].role, Role::User);
        assert!(msgs[1].content.starts_with("[Session Summary]"));
    }

    #[tokio::test]
    async fn compact_preserves_system_prompt() {
        let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("summary here")]);
        let mut msgs = vec![
            Message::system("Important system instructions"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
        ];
        let config = ContextManagerConfig {
            compact_preserve_tokens: 5,
            ..Default::default()
        };

        compact(&mut msgs, &config, &mock).await.unwrap();

        assert_eq!(msgs[0].content, "Important system instructions");
    }

    #[tokio::test]
    async fn compact_noop_when_split_at_1() {
        let mock = MockLlmClient::new("mock");
        let mut msgs = vec![Message::system("sys"), Message::user("u1")];
        let config = ContextManagerConfig {
            compact_preserve_tokens: 1_000_000, // everything preserved
            ..Default::default()
        };

        let stats = compact(&mut msgs, &config, &mock).await.unwrap();

        assert_eq!(stats.messages_replaced, 0);
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn compact_skips_on_empty_summary() {
        let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("")]);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
        ];
        let original_len = msgs.len();
        let config = ContextManagerConfig {
            compact_preserve_tokens: 5,
            ..Default::default()
        };

        let stats = compact(&mut msgs, &config, &mock).await.unwrap();

        assert_eq!(stats.messages_replaced, 0);
        assert_eq!(msgs.len(), original_len); // unchanged
    }

    #[tokio::test]
    async fn compact_skips_on_whitespace_only_summary() {
        let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("   \n\n  ")]);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
        ];
        let original_len = msgs.len();
        let config = ContextManagerConfig {
            compact_preserve_tokens: 5,
            ..Default::default()
        };

        let stats = compact(&mut msgs, &config, &mock).await.unwrap();

        assert_eq!(stats.messages_replaced, 0);
        assert_eq!(msgs.len(), original_len);
    }

    #[tokio::test]
    async fn compact_propagates_llm_error() {
        let mock = MockLlmClient::from_steps("mock", vec![MockStep::error("LLM is down")]);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
        ];
        let config = ContextManagerConfig {
            compact_preserve_tokens: 5,
            ..Default::default()
        };

        let result = compact(&mut msgs, &config, &mock).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compact_preserves_recent_messages() {
        // Build a conversation with measurable token sizes.
        let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("summary of old work")]);
        let old_content = "old work ".repeat(500); // ~4500 bytes ≈ 1125 tokens
        let mut msgs = vec![
            Message::system("sys"),
            Message::user(&old_content),
            Message::assistant(&old_content),
            Message::user("recent question"),
            Message::assistant("recent answer"),
        ];
        let config = ContextManagerConfig {
            compact_preserve_tokens: 20, // enough for the last 2 messages
            ..Default::default()
        };

        let stats = compact(&mut msgs, &config, &mock).await.unwrap();

        assert!(stats.messages_replaced > 0);
        // The recent messages should be at the tail.
        let last = &msgs[msgs.len() - 1];
        assert_eq!(last.content, "recent answer");
    }

    // ======================================================================
    // compact_was_effective
    // ======================================================================

    #[test]
    fn compact_was_effective_good_reduction() {
        let stats = CompactStats {
            messages_replaced: 10,
            tokens_before: 100_000,
            tokens_after: 30_000, // 30% = ratio 0.3 < 0.7
            summary_length: 500,
        };
        assert!(compact_was_effective(&stats));
    }

    #[test]
    fn compact_was_effective_poor_reduction() {
        let stats = CompactStats {
            messages_replaced: 10,
            tokens_before: 100_000,
            tokens_after: 90_000, // 90% = ratio 0.9 >= 0.7
            summary_length: 500,
        };
        assert!(!compact_was_effective(&stats));
    }

    #[test]
    fn compact_was_effective_no_messages_replaced() {
        let stats = CompactStats {
            messages_replaced: 0,
            tokens_before: 100_000,
            tokens_after: 100_000,
            summary_length: 0,
        };
        assert!(!compact_was_effective(&stats));
    }

    #[test]
    fn compact_was_effective_zero_tokens_before() {
        let stats = CompactStats {
            messages_replaced: 5,
            tokens_before: 0,
            tokens_after: 0,
            summary_length: 100,
        };
        assert!(!compact_was_effective(&stats));
    }

    // ======================================================================
    // Integration: prune reduces estimate
    // ======================================================================

    #[test]
    fn prune_reduces_token_estimate() {
        let big_content = "x".repeat(40_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::tool_result("c1", &big_content),
            Message::tool_result("c2", &big_content),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];
        let before = estimate_tokens(&msgs);
        let config = ContextManagerConfig {
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            ..Default::default()
        };
        let stats = prune(&mut msgs, &config);
        let after = estimate_tokens(&msgs);

        assert!(stats.applied);
        assert!(after < before);
        // Reduction should be significant (80k bytes → ~4k bytes for tool results).
        assert!(before - after > 15_000);
    }

    #[tokio::test]
    async fn compact_then_estimate_shows_reduction() {
        let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("brief summary")]);
        let big_content = "x".repeat(10_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user(&big_content),
            Message::assistant(&big_content),
            Message::user("u2"),
            Message::assistant("a2"),
        ];
        let before = estimate_tokens(&msgs);
        let config = ContextManagerConfig {
            compact_preserve_tokens: 20,
            ..Default::default()
        };
        let stats = compact(&mut msgs, &config, &mock).await.unwrap();
        let after = estimate_tokens(&msgs);

        assert!(stats.messages_replaced > 0);
        assert!(after < before);
    }

    // ======================================================================
    // End-to-end scenario
    // ======================================================================

    #[tokio::test]
    async fn full_scenario_prune_avoids_compact() {
        // A conversation that would trigger compact, but pruning reduces enough.
        let big_tool = "x".repeat(100_000);
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant_with_tool_calls(
                None,
                vec![ToolCall {
                    id: "c1".to_string(),
                    name: "bash".to_string(),
                    arguments: json!({"cmd": "cat big_file.txt"}),
                }],
            ),
            Message::tool_result("c1", &big_tool),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
        ];

        let config = ContextManagerConfig {
            context_window: 128_000,
            compact_trigger_ratio: 0.90,
            prune_protected_turns: 1,
            prune_tool_max: 2048,
            min_prune_savings_tokens: 100,
            compact_preserve_tokens: 20_000,
        };

        // Before prune, should_compact would be true.
        let est_before = estimate_tokens(&msgs);
        // After prune, should_compact should be false.
        let stats = prune(&mut msgs, &config);
        assert!(stats.applied);
        let est_after = estimate_tokens(&msgs);

        assert!(est_after < est_before);
        assert!(
            !should_compact(est_after, &config),
            "after prune, should not need compact"
        );
    }
}
