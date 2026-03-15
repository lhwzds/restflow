use crate::ErrorPayload;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum TestEvent {
        Session,
    }

    #[test]
    fn stream_event_round_trips() {
        let frame = StreamEnvelope::Event {
            event: TestEvent::Session,
        };

        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, frame);
        assert!(encoded.contains("stream_type"));
    }

    #[test]
    fn stream_error_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::error_with_details(
            500,
            "boom",
            Some(serde_json::json!({ "scope": "stream" })),
        );

        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, frame);
    }

    #[test]
    fn stream_start_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::Start {
            stream_id: "stream-1".to_string(),
        };
        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn stream_ack_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::Ack {
            content: "ack".to_string(),
        };
        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn stream_data_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::Data {
            content: "data".to_string(),
        };
        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn stream_tool_call_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::ToolCall {
            id: "call-1".to_string(),
            name: "search".to_string(),
            arguments: serde_json::json!({ "q": "restflow" }),
        };
        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn stream_tool_result_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::ToolResult {
            id: "call-1".to_string(),
            result: "done".to_string(),
            success: true,
        };
        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn stream_done_round_trips() {
        let frame = StreamEnvelope::<TestEvent>::Done {
            total_tokens: Some(12),
        };
        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: StreamEnvelope<TestEvent> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, frame);
    }
}
