use crate::models::chat_session::ExecutionStepInfo;
use crate::models::{ToolTrace, ToolTraceEvent};
use crate::runtime::trace::{
    MAX_TRACE_EVENT_TEXT_CHARS, RestflowTrace, append_restflow_model_switch, append_trace_event,
    normalize_trace_payload, sanitize_trace_secrets, truncate_trace_text,
};
use crate::storage::{ExecutionTraceStorage, ToolTraceStorage};
use async_trait::async_trait;
use restflow_ai::agent::StreamEmitter;
use restflow_trace::TraceEvent;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

fn normalize_full_payload(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = sanitize_trace_secrets(trimmed);
    match serde_json::from_str::<serde_json::Value>(&sanitized) {
        Ok(json) => Some(json.to_string()),
        Err(_) => Some(sanitized),
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn maybe_persist_full_output(
    session_id: &str,
    turn_id: &str,
    tool_call_id: &str,
    tool_name: &str,
    full_output: &str,
) -> Option<String> {
    if full_output.chars().count() <= MAX_TRACE_EVENT_TEXT_CHARS {
        return None;
    }

    let traces_dir = crate::paths::ensure_restflow_dir()
        .ok()?
        .join("traces")
        .join(sanitize_path_component(session_id))
        .join(sanitize_path_component(turn_id));
    if std::fs::create_dir_all(&traces_dir).is_err() {
        return None;
    }

    let file_name = format!(
        "{}-{}.txt",
        sanitize_path_component(tool_name),
        sanitize_path_component(tool_call_id)
    );
    let path = traces_dir.join(file_name);
    if std::fs::write(&path, full_output).is_err() {
        return None;
    }

    Some(path.to_string_lossy().to_string())
}

/// Build execution step info from completed tool traces.
pub fn build_execution_steps(traces: &[ToolTrace]) -> Vec<ExecutionStepInfo> {
    traces
        .iter()
        .filter(|t| t.event_type == ToolTraceEvent::ToolCallCompleted)
        .map(|t| {
            let status = if t.success.unwrap_or(true) {
                "completed"
            } else {
                "failed"
            };
            let tool_name = t
                .tool_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("unknown_tool");
            let mut step =
                ExecutionStepInfo::new("tool_call", tool_name.to_string()).with_status(status);
            if let Some(ms) = t.duration_ms {
                step = step.with_duration(ms);
            }
            step
        })
        .collect()
}

/// Stream emitter that forwards events and persists tool call records to storage.
pub struct ToolTraceEmitter {
    inner: Box<dyn StreamEmitter>,
    trace_storage: ToolTraceStorage,
    execution_trace_storage: Option<ExecutionTraceStorage>,
    trace: RestflowTrace,
    tool_start_times: HashMap<String, Instant>,
    tool_inputs: HashMap<String, Option<String>>,
    _base_dir: Option<PathBuf>,
}

impl ToolTraceEmitter {
    /// Create a new tool-trace emitter.
    pub fn new(
        inner: Box<dyn StreamEmitter>,
        trace_storage: ToolTraceStorage,
        trace: RestflowTrace,
    ) -> Self {
        Self {
            inner,
            trace_storage,
            execution_trace_storage: None,
            trace,
            tool_start_times: HashMap::new(),
            tool_inputs: HashMap::new(),
            _base_dir: crate::paths::ensure_restflow_dir().ok(),
        }
    }

    /// Attach execution trace storage for dual-write.
    pub fn with_execution_trace_storage(
        mut self,
        execution_trace_storage: ExecutionTraceStorage,
    ) -> Self {
        self.execution_trace_storage = Some(execution_trace_storage);
        self
    }
}

#[async_trait]
impl StreamEmitter for ToolTraceEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        self.inner.emit_text_delta(text).await;
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        self.inner.emit_thinking_delta(text).await;
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.inner.emit_tool_call_start(id, name, arguments).await;
        self.tool_start_times.insert(id.to_string(), Instant::now());
        let normalized_input = normalize_trace_payload(arguments);
        self.tool_inputs
            .insert(id.to_string(), normalized_input.clone());
        let event = TraceEvent::tool_call_started(
            self.trace.clone(),
            id.to_string(),
            name.to_string(),
            normalized_input,
        );
        append_trace_event(
            &self.trace_storage,
            self.execution_trace_storage.as_ref(),
            &event,
        );
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        self.inner
            .emit_tool_call_result(id, name, result, success)
            .await;

        let duration_ms = self
            .tool_start_times
            .remove(id)
            .map(|start| start.elapsed().as_millis() as u64);
        let input_summary = self.tool_inputs.remove(id).flatten();
        let full_output = normalize_full_payload(result);
        let output = full_output
            .as_deref()
            .map(|value| truncate_trace_text(value, MAX_TRACE_EVENT_TEXT_CHARS));
        let output_ref = full_output.as_deref().and_then(|value| {
            maybe_persist_full_output(&self.trace.session_id, &self.trace.turn_id, id, name, value)
        });
        let error = if success {
            None
        } else {
            Some(
                output
                    .clone()
                    .unwrap_or_else(|| "tool call failed".to_string()),
            )
        };
        let event = TraceEvent::tool_call_completed(
            self.trace.clone(),
            id.to_string(),
            name.to_string(),
            input_summary,
            output,
            output_ref,
            success,
            duration_ms,
            error,
        );
        append_trace_event(
            &self.trace_storage,
            self.execution_trace_storage.as_ref(),
            &event,
        );
    }

    #[allow(clippy::too_many_arguments)]
    async fn emit_llm_call(
        &mut self,
        model: &str,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        total_tokens: Option<u32>,
        cost_usd: Option<f64>,
        duration_ms: Option<u64>,
        is_reasoning: Option<bool>,
        message_count: Option<u32>,
    ) {
        self.inner
            .emit_llm_call(
                model,
                input_tokens,
                output_tokens,
                total_tokens,
                cost_usd,
                duration_ms,
                is_reasoning,
                message_count,
            )
            .await;

        let event = TraceEvent::llm_call(
            self.trace.clone(),
            model.to_string(),
            input_tokens,
            output_tokens,
            total_tokens,
            cost_usd,
            duration_ms,
            is_reasoning,
            message_count,
        );
        append_trace_event(
            &self.trace_storage,
            self.execution_trace_storage.as_ref(),
            &event,
        );
    }

    async fn emit_complete(&mut self) {
        self.inner.emit_complete().await;
    }

    async fn emit_model_switch(&mut self, from_model: &str, to_model: &str, reason: Option<&str>) {
        self.inner
            .emit_model_switch(from_model, to_model, reason)
            .await;
        append_restflow_model_switch(
            self.execution_trace_storage.as_ref(),
            &self.trace,
            from_model,
            to_model,
            reason,
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ExecutionTraceCategory, ToolCallCompletion};
    use crate::storage::ExecutionTraceStorage;
    use redb::Database;
    use restflow_ai::agent::NullEmitter;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_storage() -> ToolTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ToolTraceStorage::new(db).expect("storage")
    }

    fn setup_execution_storage() -> ExecutionTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("execution.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ExecutionTraceStorage::new(db).expect("execution storage")
    }

    #[tokio::test]
    async fn test_tool_trace_emitter_writes_tool_events() {
        let storage = setup_storage();
        let mut emitter = ToolTraceEmitter::new(
            Box::new(NullEmitter),
            storage.clone(),
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
        );

        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        let events = storage
            .list_by_session_turn("session-1", "run-run-1", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].tool_name.as_deref(), Some("bash"));
        assert_eq!(events[1].success, Some(true));
    }

    #[tokio::test]
    async fn test_tool_trace_emitter_writes_llm_events_to_execution_trace() {
        let storage = setup_storage();
        let execution_storage = setup_execution_storage();
        let mut emitter = ToolTraceEmitter::new(
            Box::new(NullEmitter),
            storage,
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
        )
        .with_execution_trace_storage(execution_storage.clone());

        emitter
            .emit_llm_call(
                "gpt-5",
                Some(10),
                Some(6),
                Some(16),
                Some(0.02),
                Some(140),
                None,
                Some(3),
            )
            .await;

        let events = execution_storage
            .query(&crate::models::ExecutionTraceQuery {
                task_id: Some("task-1".to_string()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, ExecutionTraceCategory::LlmCall);
        assert_eq!(
            events[0]
                .llm_call
                .as_ref()
                .map(|trace| trace.model.as_str()),
            Some("gpt-5")
        );
        assert_eq!(events[0].subflow_path, vec!["run-1".to_string()]);
    }

    #[tokio::test]
    async fn test_tool_trace_emitter_writes_model_switch_events_to_execution_trace() {
        let storage = setup_storage();
        let execution_storage = setup_execution_storage();
        let mut emitter = ToolTraceEmitter::new(
            Box::new(NullEmitter),
            storage,
            RestflowTrace::new("run-1", "session-1", "task-1", "agent-1"),
        )
        .with_execution_trace_storage(execution_storage.clone());

        emitter
            .emit_model_switch(
                "minimax-coding-plan-m2-5-highspeed",
                "minimax-coding-plan-m2-5",
                Some("failover"),
            )
            .await;

        let events = execution_storage
            .query(&crate::models::ExecutionTraceQuery {
                task_id: Some("task-1".to_string()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, ExecutionTraceCategory::ModelSwitch);
        assert_eq!(
            events[0].model_switch.as_ref().map(|trace| (
                trace.from_model.as_str(),
                trace.to_model.as_str(),
                trace.reason.as_deref(),
                trace.success
            )),
            Some((
                "minimax-coding-plan-m2-5-highspeed",
                "minimax-coding-plan-m2-5",
                Some("failover"),
                true
            ))
        );
    }

    #[test]
    fn test_build_execution_steps_filters_and_maps_completed_tool_calls() {
        let traces = vec![
            ToolTrace::turn_started("session-1", "turn-1"),
            ToolTrace::tool_call_completed(
                "session-1",
                "turn-1",
                "call-1",
                "web_search",
                ToolCallCompletion {
                    output: None,
                    output_ref: None,
                    success: true,
                    duration_ms: Some(1250),
                    error: None,
                },
            ),
            ToolTrace::tool_call_completed(
                "session-1",
                "turn-1",
                "call-2",
                "transcribe",
                ToolCallCompletion {
                    output: None,
                    output_ref: None,
                    success: false,
                    duration_ms: None,
                    error: Some("tool failed".to_string()),
                },
            ),
        ];

        let steps = build_execution_steps(&traces);
        assert_eq!(steps.len(), 2);

        assert_eq!(steps[0].step_type, "tool_call");
        assert_eq!(steps[0].name, "web_search");
        assert_eq!(steps[0].status, "completed");
        assert_eq!(steps[0].duration_ms, Some(1250));

        assert_eq!(steps[1].step_type, "tool_call");
        assert_eq!(steps[1].name, "transcribe");
        assert_eq!(steps[1].status, "failed");
        assert_eq!(steps[1].duration_ms, None);
    }

    #[test]
    fn test_build_execution_steps_uses_unknown_tool_when_name_missing_or_blank() {
        let mut missing_name = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-1",
            "placeholder",
            ToolCallCompletion {
                output: None,
                output_ref: None,
                success: true,
                duration_ms: None,
                error: None,
            },
        );
        missing_name.tool_name = None;

        let mut blank_name = ToolTrace::tool_call_completed(
            "session-1",
            "turn-1",
            "call-2",
            "placeholder",
            ToolCallCompletion {
                output: None,
                output_ref: None,
                success: false,
                duration_ms: Some(12),
                error: Some("failed".to_string()),
            },
        );
        blank_name.tool_name = Some("   ".to_string());

        let steps = build_execution_steps(&[missing_name, blank_name]);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "unknown_tool");
        assert_eq!(steps[1].name, "unknown_tool");
    }
}
