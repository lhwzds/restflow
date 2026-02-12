//! Execution resource tracking and guardrails for agent runs.
//!
//! Provides [`ResourceTracker`] which is checked before every tool execution
//! batch, preventing runaway agents with clear, typed error messages.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Configurable limits for a single agent run.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum total tool calls per run. 0 = disabled.
    pub max_tool_calls: usize,
    /// Maximum wall-clock time per run. Zero duration = disabled.
    pub max_wall_clock: Duration,
    /// Maximum sub-agent nesting depth. 0 = disabled.
    pub max_depth: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_tool_calls: 200,
            max_wall_clock: Duration::from_secs(30 * 60), // 30 minutes
            max_depth: 20,
        }
    }
}

/// Runtime counter checked before every tool execution batch.
pub struct ResourceTracker {
    limits: ResourceLimits,
    start_time: Instant,
    tool_call_count: AtomicUsize,
    current_depth: usize,
}

impl ResourceTracker {
    /// Create a new tracker at depth 0.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            start_time: Instant::now(),
            tool_call_count: AtomicUsize::new(0),
            current_depth: 0,
        }
    }

    /// Create a child tracker for a sub-agent at the given depth.
    pub fn with_depth(limits: ResourceLimits, depth: usize) -> Self {
        Self {
            limits,
            start_time: Instant::now(),
            tool_call_count: AtomicUsize::new(0),
            current_depth: depth,
        }
    }

    /// Check all enabled limits. Returns `Err` on the first violation.
    pub fn check(&self) -> std::result::Result<(), ResourceError> {
        self.check_tool_calls()?;
        self.check_wall_clock()?;
        self.check_depth()?;
        Ok(())
    }

    /// Check only the wall-clock limit (useful before LLM calls).
    pub fn check_wall_clock(&self) -> std::result::Result<(), ResourceError> {
        let limit = self.limits.max_wall_clock;
        if limit.is_zero() {
            return Ok(());
        }
        let elapsed = self.start_time.elapsed();
        if elapsed > limit {
            return Err(ResourceError::WallClockExceeded { limit, elapsed });
        }
        Ok(())
    }

    /// Record that `count` tool calls were executed.
    pub fn record_tool_calls(&self, count: usize) {
        self.tool_call_count.fetch_add(count, Ordering::Relaxed);
    }

    /// Return a snapshot of current resource usage.
    pub fn usage_snapshot(&self) -> ResourceUsage {
        ResourceUsage {
            tool_calls: self.tool_call_count.load(Ordering::Relaxed),
            wall_clock: self.start_time.elapsed(),
            depth: self.current_depth,
        }
    }

    fn check_tool_calls(&self) -> std::result::Result<(), ResourceError> {
        let limit = self.limits.max_tool_calls;
        if limit == 0 {
            return Ok(());
        }
        let actual = self.tool_call_count.load(Ordering::Relaxed);
        if actual >= limit {
            return Err(ResourceError::ToolCallsExceeded { limit, actual });
        }
        Ok(())
    }

    fn check_depth(&self) -> std::result::Result<(), ResourceError> {
        let limit = self.limits.max_depth;
        if limit == 0 {
            return Ok(());
        }
        if self.current_depth >= limit {
            return Err(ResourceError::DepthExceeded {
                limit,
                actual: self.current_depth,
            });
        }
        Ok(())
    }
}

/// Typed error describing which resource limit was exceeded.
#[derive(Debug, Clone)]
pub enum ResourceError {
    ToolCallsExceeded { limit: usize, actual: usize },
    WallClockExceeded { limit: Duration, elapsed: Duration },
    DepthExceeded { limit: usize, actual: usize },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceError::ToolCallsExceeded { limit, actual } => {
                write!(
                    f,
                    "Exceeded tool call limit: {} calls (limit: {})",
                    actual, limit
                )
            }
            ResourceError::WallClockExceeded { limit, elapsed } => {
                write!(
                    f,
                    "Exceeded wall-clock limit: {:.1}s elapsed (limit: {:.1}s)",
                    elapsed.as_secs_f64(),
                    limit.as_secs_f64()
                )
            }
            ResourceError::DepthExceeded { limit, actual } => {
                write!(
                    f,
                    "Exceeded depth limit: depth {} (limit: {})",
                    actual, limit
                )
            }
        }
    }
}

impl std::error::Error for ResourceError {}

/// Point-in-time snapshot of resource usage for reporting.
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub tool_calls: usize,
    pub wall_clock: Duration,
    pub depth: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_default_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_tool_calls, 200);
        assert_eq!(limits.max_wall_clock, Duration::from_secs(30 * 60));
        assert_eq!(limits.max_depth, 20);
    }

    #[test]
    fn test_tracker_records_tool_calls() {
        let tracker = ResourceTracker::new(ResourceLimits::default());
        tracker.record_tool_calls(5);
        tracker.record_tool_calls(3);
        assert_eq!(tracker.usage_snapshot().tool_calls, 8);
    }

    #[test]
    fn test_tool_call_limit_exceeded() {
        let limits = ResourceLimits {
            max_tool_calls: 10,
            ..Default::default()
        };
        let tracker = ResourceTracker::new(limits);
        tracker.record_tool_calls(10);
        let err = tracker.check().unwrap_err();
        assert!(matches!(
            err,
            ResourceError::ToolCallsExceeded {
                limit: 10,
                actual: 10
            }
        ));
    }

    #[test]
    fn test_tool_call_limit_not_exceeded() {
        let limits = ResourceLimits {
            max_tool_calls: 10,
            ..Default::default()
        };
        let tracker = ResourceTracker::new(limits);
        tracker.record_tool_calls(9);
        assert!(tracker.check().is_ok());
    }

    #[test]
    fn test_disabled_limit_zero() {
        let limits = ResourceLimits {
            max_tool_calls: 0,
            max_wall_clock: Duration::ZERO,
            max_depth: 0,
        };
        let tracker = ResourceTracker::new(limits);
        tracker.record_tool_calls(999);
        assert!(tracker.check().is_ok());
    }

    #[test]
    fn test_depth_exceeded() {
        let limits = ResourceLimits {
            max_depth: 5,
            ..Default::default()
        };
        let tracker = ResourceTracker::with_depth(limits, 5);
        let err = tracker.check().unwrap_err();
        assert!(matches!(
            err,
            ResourceError::DepthExceeded {
                limit: 5,
                actual: 5
            }
        ));
    }

    #[test]
    fn test_depth_within_limit() {
        let limits = ResourceLimits {
            max_depth: 5,
            ..Default::default()
        };
        let tracker = ResourceTracker::with_depth(limits, 4);
        assert!(tracker.check().is_ok());
    }

    #[test]
    fn test_wall_clock_fresh_tracker_ok() {
        let tracker = ResourceTracker::new(ResourceLimits::default());
        assert!(tracker.check_wall_clock().is_ok());
    }

    #[test]
    fn test_usage_snapshot() {
        let tracker = ResourceTracker::with_depth(ResourceLimits::default(), 3);
        tracker.record_tool_calls(7);
        let snap = tracker.usage_snapshot();
        assert_eq!(snap.tool_calls, 7);
        assert_eq!(snap.depth, 3);
        assert!(snap.wall_clock < Duration::from_secs(1));
    }

    #[test]
    fn test_resource_error_display() {
        let err = ResourceError::ToolCallsExceeded {
            limit: 200,
            actual: 201,
        };
        let msg = err.to_string();
        assert!(msg.contains("201"));
        assert!(msg.contains("200"));

        let err = ResourceError::DepthExceeded {
            limit: 20,
            actual: 21,
        };
        let msg = err.to_string();
        assert!(msg.contains("21"));
        assert!(msg.contains("20"));
    }

    #[test]
    fn test_concurrent_tool_call_recording() {
        let tracker = std::sync::Arc::new(ResourceTracker::new(ResourceLimits::default()));
        let mut handles = vec![];
        for _ in 0..10 {
            let t = tracker.clone();
            handles.push(thread::spawn(move || {
                t.record_tool_calls(1);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(tracker.usage_snapshot().tool_calls, 10);
    }
}
