//! Compatibility wrapper for execution trace types.
//!
//! This module re-exports types from `execution_trace` with legacy naming
//! for backward compatibility. New code should use `execution_trace` directly.
//!
//! # Migration Guide
//!
//! | Legacy Name (audit)        | New Name (execution_trace)     |
//! |----------------------------|--------------------------------|
//! | `AuditEvent`               | `ExecutionTraceEvent`          |
//! | `AuditEventCategory`       | `ExecutionTraceCategory`       |
//! | `AuditEventSource`         | `ExecutionTraceSource`         |
//! | `AuditQuery`               | `ExecutionTraceQuery`          |
//! | `AuditStats`               | `ExecutionTraceStats`          |
//! | `AuditTimeRange`           | `ExecutionTraceTimeRange`      |
//! | `LlmCallAudit`             | `LlmCallTrace`                 |
//! | `ToolCallAudit`            | `ToolCallTrace`                |
//! | `ModelSwitchAudit`         | `ModelSwitchTrace`             |
//! | `LifecycleAudit`           | `LifecycleTrace`               |
//! | `MessageAudit`             | `MessageTrace`                 |

// Re-export all types from execution_trace
pub use super::execution_trace::{
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery, ExecutionTraceSource,
    ExecutionTraceStats, ExecutionTraceTimeRange, LifecycleTrace, LlmCallTrace, MessageTrace,
    ModelSwitchTrace, ToolCallTrace,
};

// Type aliases for backward compatibility
/// Legacy alias for [`ExecutionTraceEvent`]
pub type AuditEvent = ExecutionTraceEvent;

/// Legacy alias for [`ExecutionTraceCategory`]
pub type AuditEventCategory = ExecutionTraceCategory;

/// Legacy alias for [`ExecutionTraceSource`]
pub type AuditEventSource = ExecutionTraceSource;

/// Legacy alias for [`ExecutionTraceQuery`]
pub type AuditQuery = ExecutionTraceQuery;

/// Legacy alias for [`ExecutionTraceStats`]
pub type AuditStats = ExecutionTraceStats;

/// Legacy alias for [`ExecutionTraceTimeRange`]
pub type AuditTimeRange = ExecutionTraceTimeRange;

/// Legacy alias for [`LlmCallTrace`]
pub type LlmCallAudit = LlmCallTrace;

/// Legacy alias for [`ToolCallTrace`]
pub type ToolCallAudit = ToolCallTrace;

/// Legacy alias for [`ModelSwitchTrace`]
pub type ModelSwitchAudit = ModelSwitchTrace;

/// Legacy alias for [`LifecycleTrace`]
pub type LifecycleAudit = LifecycleTrace;

/// Legacy alias for [`MessageTrace`]
pub type MessageAudit = MessageTrace;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::execution_trace_builders;

    #[test]
    fn test_legacy_type_aliases() {
        // Verify that legacy type aliases work correctly
        let event: AuditEvent = execution_trace_builders::llm_call(
            "task-123",
            "agent-456",
            LlmCallAudit {
                model: "claude-3-5-sonnet-20241022".to_string(),
                input_tokens: Some(1000),
                output_tokens: Some(500),
                total_tokens: Some(1500),
                cost_usd: Some(0.01),
                duration_ms: Some(1500),
                is_reasoning: Some(false),
                message_count: Some(10),
            },
        );

        assert_eq!(event.task_id, "task-123");
        assert_eq!(event.category, AuditEventCategory::LlmCall);
    }
}
