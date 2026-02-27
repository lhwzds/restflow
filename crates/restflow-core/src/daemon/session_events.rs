use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::broadcast;

const BUFFER_CAPACITY: usize = 256;

/// Event types for chat session changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
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
}
