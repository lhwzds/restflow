//! Stuck detection for agent ReAct loops.
//!
//! Detects when an agent repeatedly calls the same tool with the same arguments,
//! indicating it is stuck in a loop. Supports two actions: nudge (inject a system
//! message) or stop (force-terminate).

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Configuration for stuck detection.
#[derive(Debug, Clone)]
pub struct StuckDetectorConfig {
    /// Number of consecutive identical tool calls to trigger detection.
    /// Default: 3.
    pub repeat_threshold: usize,
    /// Maximum recent tool calls to track. Default: 10.
    pub window_size: usize,
    /// Whether to inject a nudge message or force-stop. Default: nudge.
    pub action: StuckAction,
}

impl Default for StuckDetectorConfig {
    fn default() -> Self {
        Self {
            repeat_threshold: 3,
            window_size: 10,
            action: StuckAction::Nudge,
        }
    }
}

/// Action to take when the agent is detected as stuck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StuckAction {
    /// Inject a system message telling the agent to try a different approach.
    Nudge,
    /// Force-stop execution with an error.
    Stop,
}

/// Information about a detected stuck state.
#[derive(Debug, Clone)]
pub struct StuckInfo {
    pub repeated_tool: String,
    pub repeat_count: usize,
    pub message: String,
}

/// Fingerprint of a tool call for comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ToolCallFingerprint {
    tool_name: String,
    args_hash: u64,
}

impl ToolCallFingerprint {
    fn new(tool_name: &str, args_json: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        args_json.hash(&mut hasher);
        Self {
            tool_name: tool_name.to_string(),
            args_hash: hasher.finish(),
        }
    }
}

/// Tracks recent tool calls and detects repetitive patterns.
pub struct StuckDetector {
    config: StuckDetectorConfig,
    recent_calls: VecDeque<ToolCallFingerprint>,
}

impl StuckDetector {
    /// Create a new stuck detector with the given configuration.
    pub fn new(config: StuckDetectorConfig) -> Self {
        Self {
            recent_calls: VecDeque::with_capacity(config.window_size),
            config,
        }
    }

    /// Return the detector's configuration.
    pub fn config(&self) -> &StuckDetectorConfig {
        &self.config
    }

    /// Record a tool call.
    pub fn record(&mut self, tool_name: &str, args_json: &str) {
        let fingerprint = ToolCallFingerprint::new(tool_name, args_json);
        if self.recent_calls.len() >= self.config.window_size {
            self.recent_calls.pop_front();
        }
        self.recent_calls.push_back(fingerprint);
    }

    /// Check if the agent is stuck (last N calls are identical).
    pub fn is_stuck(&self) -> Option<StuckInfo> {
        let threshold = self.config.repeat_threshold;
        if self.recent_calls.len() < threshold {
            return None;
        }

        // Check if the last `threshold` calls are all identical
        let last = self.recent_calls.back()?;
        let tail_start = self.recent_calls.len() - threshold;
        let all_same = self
            .recent_calls
            .iter()
            .skip(tail_start)
            .all(|fp| fp == last);

        if all_same {
            Some(StuckInfo {
                repeated_tool: last.tool_name.clone(),
                repeat_count: threshold,
                message: format!(
                    "You appear to be stuck: you have called '{}' {} times consecutively \
                     with the same arguments. Please try a different approach or tool.",
                    last.tool_name, threshold
                ),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_stuck_with_varied_calls() {
        let mut detector = StuckDetector::new(StuckDetectorConfig::default());
        detector.record("bash", r#"{"command":"ls"}"#);
        detector.record("file", r#"{"path":"/tmp"}"#);
        detector.record("web_search", r#"{"query":"rust"}"#);
        assert!(detector.is_stuck().is_none());
    }

    #[test]
    fn test_stuck_on_repeated_same_call() {
        let mut detector = StuckDetector::new(StuckDetectorConfig::default());
        let args = r#"{"command":"ls /tmp"}"#;
        detector.record("bash", args);
        detector.record("bash", args);
        assert!(detector.is_stuck().is_none()); // only 2, threshold is 3

        detector.record("bash", args);
        let info = detector.is_stuck().expect("should be stuck");
        assert_eq!(info.repeated_tool, "bash");
        assert_eq!(info.repeat_count, 3);
        assert!(info.message.contains("bash"));
    }

    #[test]
    fn test_stuck_on_same_name_different_args() {
        let mut detector = StuckDetector::new(StuckDetectorConfig::default());
        detector.record("bash", r#"{"command":"ls"}"#);
        detector.record("bash", r#"{"command":"pwd"}"#);
        detector.record("bash", r#"{"command":"whoami"}"#);
        assert!(detector.is_stuck().is_none());
    }

    #[test]
    fn test_window_size_respected() {
        let config = StuckDetectorConfig {
            repeat_threshold: 3,
            window_size: 4,
            ..Default::default()
        };
        let mut detector = StuckDetector::new(config);
        let args = r#"{"x":1}"#;

        // Fill: [bash, bash, other, bash]
        detector.record("bash", args);
        detector.record("bash", args);
        detector.record("other", r#"{"y":2}"#);
        detector.record("bash", args);
        assert!(detector.is_stuck().is_none()); // not 3 consecutive

        // Push one more identical — window becomes [bash, other, bash, bash]
        detector.record("bash", args);
        assert!(detector.is_stuck().is_none()); // only 2 consecutive at tail

        // Push another — window becomes [other, bash, bash, bash]
        detector.record("bash", args);
        let info = detector.is_stuck().expect("should be stuck now");
        assert_eq!(info.repeated_tool, "bash");
    }

    #[test]
    fn test_threshold_configurable() {
        let config = StuckDetectorConfig {
            repeat_threshold: 2,
            window_size: 10,
            action: StuckAction::Stop,
        };
        let mut detector = StuckDetector::new(config);
        let args = r#"{"cmd":"echo hi"}"#;

        detector.record("bash", args);
        assert!(detector.is_stuck().is_none());

        detector.record("bash", args);
        assert!(detector.is_stuck().is_some());
    }

    #[test]
    fn test_nudge_message_content() {
        let mut detector = StuckDetector::new(StuckDetectorConfig::default());
        let args = r#"{"query":"test"}"#;
        for _ in 0..3 {
            detector.record("web_search", args);
        }
        let info = detector.is_stuck().unwrap();
        assert!(info.message.contains("web_search"));
        assert!(info.message.contains("3 times"));
        assert!(info.message.contains("different approach"));
    }

    #[test]
    fn test_disabled_detection() {
        // If threshold is very high, detection effectively disabled
        let config = StuckDetectorConfig {
            repeat_threshold: 1000,
            window_size: 10,
            ..Default::default()
        };
        let mut detector = StuckDetector::new(config);
        let args = r#"{"x":1}"#;
        for _ in 0..10 {
            detector.record("bash", args);
        }
        assert!(detector.is_stuck().is_none());
    }

    #[test]
    fn test_default_config() {
        let config = StuckDetectorConfig::default();
        assert_eq!(config.repeat_threshold, 3);
        assert_eq!(config.window_size, 10);
        assert_eq!(config.action, StuckAction::Nudge);
    }
}
