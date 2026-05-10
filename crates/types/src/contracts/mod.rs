//! Shared boundary contracts used across transport and app layers.

pub mod request;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

pub use request::IpcRequest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    ConfirmationRequired,
    NotFound,
    Conflict,
    Unauthorized,
    Forbidden,
    RateLimit,
    Timeout,
    Protocol,
    Internal,
}

impl ErrorKind {
    pub fn from_code(code: i32) -> Self {
        match code {
            400 => Self::Validation,
            428 => Self::ConfirmationRequired,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            408 | 504 => Self::Timeout,
            409 => Self::Conflict,
            429 => Self::RateLimit,
            code if code < 0 => Self::Protocol,
            _ => Self::Internal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorPayload {
    pub code: i32,
    pub kind: ErrorKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ErrorPayload {
    pub fn new(code: i32, message: impl Into<String>, details: Option<Value>) -> Self {
        Self {
            code,
            kind: ErrorKind::from_code(code),
            message: message.into(),
            details,
        }
    }

    pub fn with_kind(
        code: i32,
        kind: ErrorKind,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self {
            code,
            kind,
            message: message.into(),
            details,
        }
    }

    pub fn not_found(what: &str) -> Self {
        Self::with_kind(
            404,
            ErrorKind::NotFound,
            format!("{} not found", what),
            None,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "response_type", content = "data")]
pub enum ResponseEnvelope<T> {
    Pong,
    Success(T),
    Error(ErrorPayload),
}

impl ResponseEnvelope<Value> {
    pub fn success<T: Serialize>(data: T) -> Self {
        match serde_json::to_value(data) {
            Ok(value) => Self::Success(value),
            Err(error) => Self::Error(ErrorPayload::with_kind(
                500,
                ErrorKind::Internal,
                "Failed to serialize response payload",
                Some(serde_json::json!({ "cause": error.to_string() })),
            )),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self::error_with_details(code, message, None)
    }

    pub fn error_with_details(
        code: i32,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self::Error(ErrorPayload::new(code, message, details))
    }

    pub fn error_payload(payload: ErrorPayload) -> Self {
        Self::Error(payload)
    }

    pub fn not_found(what: &str) -> Self {
        Self::Error(ErrorPayload::not_found(what))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFrame {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultFrame {
    pub id: String,
    pub result: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "stream_type", content = "data")]
pub enum StreamEnvelope<TEvent> {
    Start {
        stream_id: String,
    },
    Ack {
        content: String,
    },
    Data {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        result: String,
        success: bool,
    },
    Event {
        event: TEvent,
    },
    Done {
        total_tokens: Option<u32>,
    },
    Error(ErrorPayload),
}

impl<TEvent> StreamEnvelope<TEvent> {
    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self::Error(ErrorPayload::new(code, message, None))
    }

    pub fn error_with_details(
        code: i32,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self::Error(ErrorPayload::new(code, message, details))
    }
}

impl<TEvent> From<ToolCallFrame> for StreamEnvelope<TEvent> {
    fn from(frame: ToolCallFrame) -> Self {
        Self::ToolCall {
            id: frame.id,
            name: frame.name,
            arguments: frame.arguments,
        }
    }
}

impl<TEvent> From<ToolResultFrame> for StreamEnvelope<TEvent> {
    fn from(frame: ToolResultFrame) -> Self {
        Self::ToolResult {
            id: frame.id,
            result: frame.result,
            success: frame.success,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatSessionEvent {
    Created { session_id: String },
    Updated { session_id: String },
    MessageAdded { session_id: String, source: String },
    Deleted { session_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcStreamEvent {
    Session(ChatSessionEvent),
}

pub type StreamFrame = StreamEnvelope<IpcStreamEvent>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionScope {
    Foreground {
        client_id: String,
        terminal_id: String,
    },
    Subagent {
        parent_run_id: String,
    },
}

impl ExecutionScope {
    pub fn foreground(client_id: impl Into<String>, terminal_id: impl Into<String>) -> Self {
        Self::Foreground {
            client_id: client_id.into(),
            terminal_id: terminal_id.into(),
        }
    }

    pub fn subagent(parent_run_id: impl Into<String>) -> Self {
        Self::Subagent {
            parent_run_id: parent_run_id.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteWithIdResponse {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveResponse {
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClearResponse {
    pub deleted: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelResponse {
    pub canceled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SteerResponse {
    pub steered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalHandledResponse {
    pub handled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeyResponse {
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptResponse {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretResponse {
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupReportResponse {
    pub chat_sessions: usize,
    pub tasks: usize,
    pub audit_events: usize,
    pub daemon_log_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcDaemonStatus {
    pub status: String,
    pub protocol_version: String,
    pub daemon_version: String,
    pub pid: u32,
    pub started_at_ms: i64,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolErrorCategory {
    Network,
    Auth,
    Config,
    Execution,
    RateLimit,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutionResult {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
    #[serde(default)]
    pub error_category: Option<ToolErrorCategory>,
    #[serde(default)]
    pub retryable: Option<bool>,
    #[serde(default)]
    pub retry_after_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum TestEvent {
        Session,
    }

    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("boom"))
        }
    }

    fn assert_roundtrip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let decoded: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, value);
    }

    #[test]
    fn public_contract_exports_are_accessible() {
        let _ = ErrorPayload::new(500, "boom", None);
        let _ = ResponseEnvelope::<Value>::Pong;
        let _ = StreamEnvelope::<()>::Done { total_tokens: None };
        let _ = OkResponse { ok: true };
        let _ = IpcRequest::Ping;
        let _ = IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: "2".to_string(),
            daemon_version: "0.4.0".to_string(),
            pid: 1,
            started_at_ms: 0,
            uptime_secs: 0,
        };
    }

    #[test]
    fn error_payload_round_trips() {
        let payload = ErrorPayload::with_kind(
            500,
            ErrorKind::Internal,
            "failed",
            Some(serde_json::json!({ "field": "agent_id" })),
        );

        assert_roundtrip(&payload);
    }

    #[test]
    fn error_kind_maps_from_code() {
        assert_eq!(ErrorKind::from_code(404), ErrorKind::NotFound);
        assert_eq!(ErrorKind::from_code(429), ErrorKind::RateLimit);
        assert_eq!(ErrorKind::from_code(428), ErrorKind::ConfirmationRequired);
        assert_eq!(ErrorKind::from_code(-2), ErrorKind::Protocol);
    }

    #[test]
    fn response_success_round_trips() {
        let response = ResponseEnvelope::<Value>::success(serde_json::json!({
            "deleted": true
        }));

        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: ResponseEnvelope<Value> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, response);
        assert!(encoded.contains("response_type"));
    }

    #[test]
    fn response_error_round_trips() {
        let response = ResponseEnvelope::<Value>::error_with_details(
            500,
            "failed",
            Some(serde_json::json!({ "error_kind": "session_policy" })),
        );

        assert_roundtrip(&response);
    }

    #[test]
    fn response_success_serialization_failure_returns_error_payload() {
        let response = ResponseEnvelope::<Value>::success(FailingSerialize);

        match response {
            ResponseEnvelope::Error(error) => {
                assert_eq!(error.code, 500);
                assert_eq!(error.kind, ErrorKind::Internal);
                assert_eq!(error.message, "Failed to serialize response payload");
                assert_eq!(error.details.unwrap()["cause"], "boom");
            }
            other => panic!("unexpected response variant: {other:?}"),
        }
    }

    #[test]
    fn stream_frames_round_trip() {
        let frames = vec![
            StreamEnvelope::<TestEvent>::Start {
                stream_id: "stream-1".to_string(),
            },
            StreamEnvelope::<TestEvent>::Ack {
                content: "ack".to_string(),
            },
            StreamEnvelope::<TestEvent>::Data {
                content: "data".to_string(),
            },
            StreamEnvelope::<TestEvent>::ToolCall {
                id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({ "q": "restflow" }),
            },
            StreamEnvelope::<TestEvent>::ToolResult {
                id: "call-1".to_string(),
                result: "done".to_string(),
                success: true,
            },
            StreamEnvelope::<TestEvent>::Event {
                event: TestEvent::Session,
            },
            StreamEnvelope::<TestEvent>::Done {
                total_tokens: Some(12),
            },
            StreamEnvelope::<TestEvent>::error_with_details(
                500,
                "boom",
                Some(serde_json::json!({ "scope": "stream" })),
            ),
        ];

        for frame in frames {
            assert_roundtrip(&frame);
        }
    }

    #[test]
    fn operation_responses_round_trip() {
        assert_roundtrip(&IdResponse {
            id: "memory-1".to_string(),
        });
        assert_roundtrip(&DeleteResponse { deleted: true });
        assert_roundtrip(&DeleteWithIdResponse {
            id: "task-1".to_string(),
            deleted: true,
        });
        assert_roundtrip(&ArchiveResponse { archived: true });
        assert_roundtrip(&ClearResponse { deleted: 3 });
        assert_roundtrip(&CancelResponse { canceled: true });
        assert_roundtrip(&SteerResponse { steered: true });
        assert_roundtrip(&ApprovalHandledResponse { handled: false });
        assert_roundtrip(&OkResponse { ok: true });
        assert_roundtrip(&SecretResponse {
            value: Some("token".to_string()),
        });
        assert_roundtrip(&ApiKeyResponse {
            api_key: "key".to_string(),
            profile_id: Some("profile-1".to_string()),
        });
        assert_roundtrip(&PromptResponse {
            prompt: "hello".to_string(),
        });
        assert_roundtrip(&IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: "2".to_string(),
            daemon_version: "0.4.0".to_string(),
            pid: 42,
            started_at_ms: 123,
            uptime_secs: 456,
        });
        assert_roundtrip(&CleanupReportResponse {
            chat_sessions: 1,
            tasks: 2,
            audit_events: 3,
            daemon_log_files: 4,
        });
    }

    #[test]
    fn tool_contracts_round_trip() {
        assert_roundtrip(&ToolDefinition {
            name: "search".to_string(),
            description: "Search documents".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }),
        });

        assert_roundtrip(&ToolExecutionResult {
            success: false,
            result: serde_json::json!({ "partial": "output" }),
            error: Some("Timed out".to_string()),
            error_category: Some(ToolErrorCategory::RateLimit),
            retryable: Some(true),
            retry_after_ms: Some(1_000),
        });
    }
}
