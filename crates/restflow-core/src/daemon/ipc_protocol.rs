use crate::daemon::session_events::ChatSessionEvent;
use crate::runtime::TaskStreamEvent;
pub use restflow_contracts::{IpcDaemonStatus, IpcRequest, ToolDefinition, ToolExecutionResult};
use restflow_contracts::{ResponseEnvelope, StreamEnvelope};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Message frame: [4 bytes length LE][JSON payload]
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
pub const IPC_PROTOCOL_VERSION: &str = "2";

pub type IpcResponse = ResponseEnvelope<Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcStreamEvent {
    BackgroundAgent(TaskStreamEvent),
    Session(ChatSessionEvent),
}

pub type StreamFrame = StreamEnvelope<IpcStreamEvent>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_request_reexport_roundtrip() {
        let request = IpcRequest::HandleTaskApproval {
            id: "task-1".to_string(),
            approved: true,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, request);
    }

    #[test]
    fn test_response_success() {
        let response = IpcResponse::success(serde_json::json!({ "id": "test-123" }));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        assert!(json.contains("response_type"));
        if let IpcResponse::Success(value) = parsed {
            assert_eq!(value["id"], "test-123");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_response_error() {
        let response = IpcResponse::error(404, "Not found");
        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        if let IpcResponse::Error(error) = parsed {
            assert_eq!(error.code, 404);
            assert_eq!(error.message, "Not found");
            assert_eq!(error.details, None);
            assert_eq!(error.kind, restflow_contracts::ErrorKind::NotFound);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_protocol_version_is_v2() {
        assert_eq!(IPC_PROTOCOL_VERSION, "2");
    }

    #[test]
    fn test_daemon_status_roundtrip() {
        let status = IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: IPC_PROTOCOL_VERSION.to_string(),
            daemon_version: "0.4.0".to_string(),
            pid: 1234,
            started_at_ms: 1_700_000_000_000,
            uptime_secs: 42,
        };

        let value = serde_json::to_value(&status).unwrap();
        let parsed: IpcDaemonStatus = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_stream_frame_start() {
        let frame = StreamFrame::Start {
            stream_id: "stream-1".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        assert!(json.contains("stream_type"));
        if let StreamFrame::Start { stream_id } = parsed {
            assert_eq!(stream_id, "stream-1");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_background_agent_stream_frame_serialization() {
        let event = TaskStreamEvent::progress(
            "agent-42",
            "notification",
            Some(100),
            Some("done".to_string()),
        );
        let frame = StreamFrame::Event {
            event: IpcStreamEvent::BackgroundAgent(event.clone()),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: StreamFrame = serde_json::from_str(&json).unwrap();

        match parsed {
            StreamFrame::Event {
                event: IpcStreamEvent::BackgroundAgent(parsed_event),
            } => {
                assert_eq!(parsed_event.task_id, event.task_id);
            }
            _ => panic!("Wrong variant"),
        }
    }
}
