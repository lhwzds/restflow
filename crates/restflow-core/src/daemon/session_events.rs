use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::broadcast;

const BUFFER_CAPACITY: usize = 256;

/// Event types for chat session changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatSessionEvent {
    Created {
        session_id: String,
    },
    Updated {
        session_id: String,
    },
    MessageAdded {
        session_id: String,
        /// Where the change originated (e.g. "workspace", "telegram", "ipc")
        source: String,
    },
    Deleted {
        session_id: String,
    },
}

fn stream_sender() -> &'static broadcast::Sender<ChatSessionEvent> {
    static SENDER: OnceLock<broadcast::Sender<ChatSessionEvent>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, _receiver) = broadcast::channel(BUFFER_CAPACITY);
        sender
    })
}

/// Publish a chat session change event to daemon subscribers.
pub fn publish_session_event(event: ChatSessionEvent) {
    let _ = stream_sender().send(event);
}

/// Subscribe to the daemon chat-session event bus.
pub fn subscribe_session_events() -> broadcast::Receiver<ChatSessionEvent> {
    stream_sender().subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_and_subscribe_session_event() {
        let mut receiver = subscribe_session_events();
        let event = ChatSessionEvent::MessageAdded {
            session_id: "session-1".to_string(),
            source: "telegram".to_string(),
        };

        publish_session_event(event);
        let received = receiver.recv().await.unwrap();

        match received {
            ChatSessionEvent::MessageAdded { session_id, source } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(source, "telegram");
            }
            _ => panic!("Wrong variant"),
        }
    }

    /// Ensure JSON uses `"type"` tag with flat fields — frontend depends on this.
    #[test]
    fn test_serialization_uses_type_tag() {
        let event = ChatSessionEvent::MessageAdded {
            session_id: "s1".to_string(),
            source: "telegram".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();

        assert_eq!(json["type"], "MessageAdded");
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["source"], "telegram");
        // Must NOT have nested "data" or "kind" keys
        assert!(json.get("kind").is_none());
        assert!(json.get("data").is_none());
    }

    #[test]
    fn test_serialization_all_variants() {
        let cases: Vec<(ChatSessionEvent, &str)> = vec![
            (
                ChatSessionEvent::Created {
                    session_id: "s1".into(),
                },
                "Created",
            ),
            (
                ChatSessionEvent::Updated {
                    session_id: "s2".into(),
                },
                "Updated",
            ),
            (
                ChatSessionEvent::MessageAdded {
                    session_id: "s3".into(),
                    source: "ipc".into(),
                },
                "MessageAdded",
            ),
            (
                ChatSessionEvent::Deleted {
                    session_id: "s4".into(),
                },
                "Deleted",
            ),
        ];

        for (event, expected_type) in cases {
            let json: serde_json::Value = serde_json::to_value(&event).unwrap();
            assert_eq!(
                json["type"], expected_type,
                "wrong type for {expected_type}"
            );
            assert!(
                json["session_id"].is_string(),
                "missing session_id for {expected_type}"
            );
        }
    }

    #[test]
    fn test_deserialization_from_frontend_format() {
        let json = r#"{"type":"MessageAdded","session_id":"abc","source":"workspace"}"#;
        let event: ChatSessionEvent = serde_json::from_str(json).unwrap();
        match event {
            ChatSessionEvent::MessageAdded { session_id, source } => {
                assert_eq!(session_id, "abc");
                assert_eq!(source, "workspace");
            }
            _ => panic!("Wrong variant"),
        }
    }
}
