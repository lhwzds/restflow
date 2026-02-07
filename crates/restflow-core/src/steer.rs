use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc};

use crate::models::SteerMessage;

/// Registry of steer channels for running tasks.
/// Each running task registers a sender; external code sends steer messages.
pub struct SteerRegistry {
    channels: RwLock<HashMap<String, mpsc::Sender<SteerMessage>>>,
}

impl SteerRegistry {
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
        }
    }

    /// Register a steer channel for a running task.
    /// Returns the receiver for the executor to poll.
    pub async fn register(&self, task_id: &str) -> mpsc::Receiver<SteerMessage> {
        let (tx, rx) = mpsc::channel(16);
        self.channels.write().await.insert(task_id.to_string(), tx);
        rx
    }

    /// Unregister when task completes.
    pub async fn unregister(&self, task_id: &str) {
        self.channels.write().await.remove(task_id);
    }

    /// Send a steer message to a running task.
    /// Returns false if task is not running or channel is full.
    pub async fn steer(&self, task_id: &str, message: SteerMessage) -> bool {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(task_id) {
            tx.try_send(message).is_ok()
        } else {
            false
        }
    }

    /// Check if a task has a steer channel (is running).
    pub async fn is_steerable(&self, task_id: &str) -> bool {
        self.channels.read().await.contains_key(task_id)
    }
}

impl Default for SteerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_steer_registry_register_unregister() {
        let registry = SteerRegistry::new();
        let _rx = registry.register("task-1").await;
        assert!(registry.is_steerable("task-1").await);

        registry.unregister("task-1").await;
        assert!(!registry.is_steerable("task-1").await);
    }

    #[tokio::test]
    async fn test_steer_message_delivery() {
        let registry = SteerRegistry::new();
        let mut rx = registry.register("task-1").await;

        let msg = SteerMessage {
            instruction: "check ETH too".into(),
            source: crate::models::SteerSource::User,
            timestamp: 0,
        };
        assert!(registry.steer("task-1", msg).await);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.instruction, "check ETH too");
    }

    #[tokio::test]
    async fn test_steer_nonexistent_task() {
        let registry = SteerRegistry::new();
        let msg = SteerMessage {
            instruction: "test".into(),
            source: crate::models::SteerSource::User,
            timestamp: 0,
        };
        assert!(!registry.steer("no-such-task", msg).await);
    }

    #[tokio::test]
    async fn test_steer_channel_capacity() {
        // Channel capacity is 16, sending 20 messages should drop overflow
        let registry = SteerRegistry::new();
        let _rx = registry.register("task-1").await; // don't consume

        for i in 0..20 {
            registry
                .steer(
                    "task-1",
                    SteerMessage {
                        instruction: format!("msg-{i}"),
                        source: crate::models::SteerSource::User,
                        timestamp: 0,
                    },
                )
                .await;
        }
        // First 16 should be queued, rest dropped (try_send behavior)
    }
}
