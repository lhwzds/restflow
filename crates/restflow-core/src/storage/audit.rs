//! Compatibility shim for execution trace storage.
//!
//! This module re-exports types from `execution_trace` for backward compatibility.
//! New code should use `ExecutionTraceStorage` directly from the `execution_trace` module.

// Re-export the primary storage type with the new name.
pub use super::execution_trace::ExecutionTraceStorage;

/// Backward-compatible type alias for ExecutionTraceStorage.
///
/// This alias is provided for compatibility with existing code that uses `AuditStorage`.
/// New code should use `ExecutionTraceStorage` directly.
pub type AuditStorage = ExecutionTraceStorage;

// Re-export tests from execution_trace module for backward compatibility
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::audit::LlmCallAudit;
    use crate::models::execution_trace_builders;

    #[test]
    fn test_audit_storage_compatibility() {
        // Verify that AuditStorage alias works correctly
        let storage = AuditStorage::in_memory().unwrap();

        let event = execution_trace_builders::llm_call(
            "task-123",
            "agent-456",
            LlmCallAudit {
                model: "claude-3-5-sonnet".to_string(),
                input_tokens: Some(1000),
                output_tokens: Some(500),
                total_tokens: Some(1500),
                cost_usd: Some(0.01),
                duration_ms: Some(1500),
                is_reasoning: Some(false),
                message_count: Some(10),
            },
        );

        storage.store(&event).unwrap();

        let query = crate::models::audit::AuditQuery {
            task_id: Some("task-123".to_string()),
            ..Default::default()
        };

        let results = storage.query(&query).unwrap();
        assert_eq!(results.len(), 1);
    }
}
