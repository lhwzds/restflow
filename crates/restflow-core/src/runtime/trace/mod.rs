//! Shared runtime helpers for RestFlow run traces.

use crate::runtime::channel::tool_trace_emitter::{
    ToolTraceEmitter, append_turn_cancelled_with_execution_and_ai_duration,
    append_turn_completed_with_execution_and_ai_duration,
    append_turn_failed_with_execution_and_ai_duration, append_turn_started_with_execution,
};
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use restflow_ai::agent::{NullEmitter, RunTraceSink, StreamEmitter};
pub use restflow_trace::{RestflowTrace, RunTraceContext, RunTraceOutcome};

fn restflow_trace_from_context(context: &RunTraceContext) -> RestflowTrace {
    RestflowTrace::from_run(
        context.run_id.clone(),
        context.actor_id.clone(),
        context.parent_run_id.clone(),
        None,
    )
}

/// Append run-started events for a canonical RestFlow trace.
pub fn append_restflow_trace_started(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
) {
    append_turn_started_with_execution(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
    );
}

/// Append run-completed events for a canonical RestFlow trace.
pub fn append_restflow_trace_completed(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    ai_duration_ms: Option<u64>,
) {
    append_turn_completed_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
        ai_duration_ms,
    );
}

/// Append run-failed events for a canonical RestFlow trace.
pub fn append_restflow_trace_failed(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    error_text: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_failed_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
        error_text,
        ai_duration_ms,
    );
}

/// Append run-cancelled events for a canonical RestFlow trace.
pub fn append_restflow_trace_cancelled(
    tool_trace_storage: &ToolTraceStorage,
    execution_trace_storage: Option<&ExecutionTraceStorage>,
    trace: &RestflowTrace,
    reason: &str,
    ai_duration_ms: Option<u64>,
) {
    append_turn_cancelled_with_execution_and_ai_duration(
        tool_trace_storage,
        execution_trace_storage,
        &trace.session_id,
        &trace.turn_id,
        &trace.execution_task_id,
        &trace.actor_id,
        reason,
        ai_duration_ms,
    );
}

/// Build a tool trace emitter bound to a canonical RestFlow trace.
pub fn build_restflow_trace_emitter(
    inner: Box<dyn StreamEmitter>,
    tool_trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
    trace: &RestflowTrace,
) -> Box<dyn StreamEmitter> {
    let emitter = ToolTraceEmitter::new(
        inner,
        tool_trace_storage,
        trace.session_id.clone(),
        trace.turn_id.clone(),
    );
    if let Some(storage) = execution_trace_storage {
        Box::new(emitter.with_execution_trace_context(
            storage,
            trace.execution_task_id.clone(),
            trace.actor_id.clone(),
        ))
    } else {
        Box::new(emitter)
    }
}

/// Run trace sink that persists lifecycle and tool-call events using
/// the existing tool/execution trace storages.
#[derive(Clone)]
pub struct ToolTraceRunSink {
    tool_trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
}

impl ToolTraceRunSink {
    pub fn new(
        tool_trace_storage: ToolTraceStorage,
        execution_trace_storage: Option<ExecutionTraceStorage>,
    ) -> Self {
        Self {
            tool_trace_storage,
            execution_trace_storage,
        }
    }
}

impl RunTraceSink for ToolTraceRunSink {
    fn on_run_started(&self, context: &RunTraceContext) {
        let trace = restflow_trace_from_context(context);
        append_restflow_trace_started(
            &self.tool_trace_storage,
            self.execution_trace_storage.as_ref(),
            &trace,
        );
    }

    fn build_run_emitter(&self, context: &RunTraceContext) -> Box<dyn StreamEmitter> {
        let trace = restflow_trace_from_context(context);
        build_restflow_trace_emitter(
            Box::new(NullEmitter),
            self.tool_trace_storage.clone(),
            self.execution_trace_storage.clone(),
            &trace,
        )
    }

    fn on_run_finished(&self, context: &RunTraceContext, outcome: &RunTraceOutcome) {
        let trace = restflow_trace_from_context(context);
        if outcome.success {
            append_restflow_trace_completed(
                &self.tool_trace_storage,
                self.execution_trace_storage.as_ref(),
                &trace,
                None,
            );
            return;
        }

        let error_text = outcome.error.as_deref().unwrap_or("Run execution failed");
        append_restflow_trace_failed(
            &self.tool_trace_storage,
            self.execution_trace_storage.as_ref(),
            &trace,
            error_text,
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ToolTraceEvent;
    use crate::storage::ToolTraceStorage;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_storage() -> ToolTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ToolTraceStorage::new(db).expect("storage")
    }

    #[tokio::test]
    async fn test_tool_trace_run_sink_writes_lifecycle_and_tool_events() {
        let storage = setup_storage();
        let sink = ToolTraceRunSink::new(storage.clone(), None);
        let context = RunTraceContext {
            run_id: "run-1".to_string(),
            actor_id: "worker".to_string(),
            parent_run_id: Some("parent-1".to_string()),
        };

        sink.on_run_started(&context);

        let mut emitter = sink.build_run_emitter(&context);
        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        sink.on_run_finished(
            &context,
            &RunTraceOutcome {
                success: true,
                error: None,
            },
        );

        let events = storage
            .list_by_session_turn("parent-1", "run-run-1", None)
            .expect("list");
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnStarted);
        assert_eq!(events[1].event_type, ToolTraceEvent::ToolCallStarted);
        assert_eq!(events[2].event_type, ToolTraceEvent::ToolCallCompleted);
        assert_eq!(events[3].event_type, ToolTraceEvent::TurnCompleted);
    }

    #[test]
    fn test_restflow_trace_records_ai_duration_separately_from_event_time() {
        let storage = setup_storage();
        let trace = RestflowTrace::new("run-a", "session-a", "task-a", "agent-a");
        append_restflow_trace_started(&storage, None, &trace);
        append_restflow_trace_completed(&storage, None, &trace, Some(321));

        let events = storage
            .list_by_session_turn("session-a", "run-run-a", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, ToolTraceEvent::TurnStarted);
        assert_eq!(events[1].event_type, ToolTraceEvent::TurnCompleted);
        assert_eq!(events[1].duration_ms, Some(321));
        assert!(events[1].created_at >= trace.created_at_ms);
    }
}
