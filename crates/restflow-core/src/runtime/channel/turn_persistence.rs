use crate::models::MessageExecution;
use crate::storage::ExecutionTraceStorage;
use crate::telemetry::build_execution_steps;
use tracing::warn;

use super::voice_transcript::enrich_voice_message_with_transcript;

/// Build persisted turn payload (execution metadata + user input text) from execution traces.
pub(crate) fn build_turn_persistence_payload(
    execution_traces: &ExecutionTraceStorage,
    session_id: &str,
    turn_id: &str,
    input: &str,
    duration_ms: u64,
    iterations: u32,
) -> (MessageExecution, String) {
    let mut execution = MessageExecution::new().complete(duration_ms, iterations);
    let events = match execution_traces.query(&crate::models::ExecutionTraceQuery {
        session_id: Some(session_id.to_string()),
        turn_id: Some(turn_id.to_string()),
        limit: Some(500),
        ..crate::models::ExecutionTraceQuery::default()
    }) {
        Ok(events) => events,
        Err(error) => {
            warn!(
                session_id = %session_id,
                turn_id = %turn_id,
                error = %error,
                "Failed to load execution traces for turn persistence payload"
            );
            Vec::new()
        }
    };
    for step in build_execution_steps(&events) {
        execution.add_step(step);
    }

    let persisted_input =
        enrich_voice_message_with_transcript(input, &events).unwrap_or_else(|| input.to_string());
    (execution, persisted_input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ExecutionTraceEvent, ToolCallPhase, ToolCallTrace};
    use crate::storage::Storage;
    use serde_json::json;
    use tempfile::tempdir;

    fn voice_input(path: &str) -> String {
        format!(
            "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: {path}\ninstruction: Use the transcribe tool with this file_path before answering."
        )
    }

    #[test]
    fn payload_includes_steps_and_transcript_when_transcribe_trace_matches() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("turn-persistence.db");
        let storage = Storage::new(db_path.to_str().expect("db path")).expect("storage");

        let session_id = "session-turn-persist";
        let turn_id = "run-turn-1";
        let file_path = "/tmp/voice-a.webm";

        let trace = restflow_trace::RestflowTrace::new("turn-1", session_id, session_id, "agent-1");
        let start = ExecutionTraceEvent::tool_call(
            session_id,
            "agent-1",
            ToolCallTrace {
                phase: ToolCallPhase::Started,
                tool_call_id: "call-1".to_string(),
                tool_name: "transcribe".to_string(),
                input: Some(json!({"file_path": file_path}).to_string()),
                input_summary: None,
                output: None,
                output_ref: None,
                success: None,
                error: None,
                duration_ms: None,
            },
        )
        .with_trace_context(&trace);
        let done = ExecutionTraceEvent::tool_call(
            session_id,
            "agent-1",
            ToolCallTrace {
                phase: ToolCallPhase::Completed,
                tool_call_id: "call-1".to_string(),
                tool_name: "transcribe".to_string(),
                input: None,
                input_summary: None,
                output: Some(json!({"text": "hello from transcript"}).to_string()),
                output_ref: None,
                success: Some(true),
                error: None,
                duration_ms: Some(35),
            },
        )
        .with_trace_context(&trace);
        storage
            .execution_traces
            .store(&start)
            .expect("append start trace");
        storage
            .execution_traces
            .store(&done)
            .expect("append done trace");

        let input = voice_input(file_path);
        let (execution, persisted_input) = build_turn_persistence_payload(
            &storage.execution_traces,
            session_id,
            turn_id,
            &input,
            128,
            7,
        );

        assert_eq!(execution.duration_ms, 128);
        assert_eq!(execution.tokens_used, 7);
        assert_eq!(execution.steps.len(), 1);
        assert_eq!(execution.steps[0].step_type, "tool_call");
        assert_eq!(execution.steps[0].name, "transcribe");
        assert_eq!(execution.steps[0].status, "completed");
        assert_eq!(execution.steps[0].duration_ms, Some(35));
        assert!(persisted_input.contains("[Transcript]"));
        assert!(persisted_input.contains("hello from transcript"));
    }

    #[test]
    fn payload_keeps_original_input_when_no_matching_trace() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("turn-persistence-empty.db");
        let storage = Storage::new(db_path.to_str().expect("db path")).expect("storage");

        let session_id = "session-turn-persist";
        let turn_id = "run-turn-empty";
        let input = voice_input("/tmp/voice-x.webm");
        let (execution, persisted_input) = build_turn_persistence_payload(
            &storage.execution_traces,
            session_id,
            turn_id,
            &input,
            20,
            0,
        );

        assert_eq!(execution.duration_ms, 20);
        assert_eq!(execution.tokens_used, 0);
        assert!(execution.steps.is_empty());
        assert_eq!(persisted_input, input);
    }

    #[test]
    fn payload_records_failed_step_without_transcript_enrichment() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("turn-persistence-failed.db");
        let storage = Storage::new(db_path.to_str().expect("db path")).expect("storage");

        let session_id = "session-turn-persist";
        let turn_id = "run-turn-failed";
        let file_path = "/tmp/voice-failed.webm";
        let input = voice_input(file_path);

        let trace =
            restflow_trace::RestflowTrace::new("turn-failed", session_id, session_id, "agent-1");
        let start = ExecutionTraceEvent::tool_call(
            session_id,
            "agent-1",
            ToolCallTrace {
                phase: ToolCallPhase::Started,
                tool_call_id: "call-2".to_string(),
                tool_name: "transcribe".to_string(),
                input: Some(json!({"file_path": file_path}).to_string()),
                input_summary: None,
                output: None,
                output_ref: None,
                success: None,
                error: None,
                duration_ms: None,
            },
        )
        .with_trace_context(&trace);
        let done = ExecutionTraceEvent::tool_call(
            session_id,
            "agent-1",
            ToolCallTrace {
                phase: ToolCallPhase::Completed,
                tool_call_id: "call-2".to_string(),
                tool_name: "transcribe".to_string(),
                input: None,
                input_summary: None,
                output: None,
                output_ref: None,
                success: Some(false),
                error: Some("decode failed".to_string()),
                duration_ms: Some(15),
            },
        )
        .with_trace_context(&trace);
        storage
            .execution_traces
            .store(&start)
            .expect("append start trace");
        storage
            .execution_traces
            .store(&done)
            .expect("append done trace");

        let (execution, persisted_input) = build_turn_persistence_payload(
            &storage.execution_traces,
            session_id,
            turn_id,
            &input,
            42,
            3,
        );

        assert_eq!(execution.steps.len(), 1);
        assert_eq!(execution.steps[0].name, "transcribe");
        assert_eq!(execution.steps[0].status, "failed");
        assert_eq!(execution.steps[0].duration_ms, Some(15));
        assert_eq!(persisted_input, input);
    }
}
