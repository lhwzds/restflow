use crate::llm::Message;

use super::constants::{CHARS_PER_TOKEN, ROLE_OVERHEAD_TOKENS};

/// Estimate tokens for a single message (bytes / CHARS_PER_TOKEN + role overhead).
pub(crate) fn estimate_message_tokens(msg: &Message) -> usize {
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

/// Exponential-moving-average calibrated token estimator.
///
/// Tracks a rolling `calibration_factor` (ratio of actual to heuristic tokens)
/// and applies it to future estimates. Also provides a compaction cooldown
/// to prevent compaction loops.
#[derive(Debug, Clone)]
pub struct TokenEstimator {
    pub(crate) calibration_factor: f64,
    pub(crate) samples: usize,
    /// Iterations remaining before compact is allowed again.
    pub(crate) compact_cooldown: usize,
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
