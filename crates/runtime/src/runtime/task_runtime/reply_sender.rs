//! Background-agent reply sender integration for the `reply` tool.
//!
//! This module wires execution-scoped reply semantics for tasks:
//! - emit a live task stream output event
//! - persist agent-originated reply messages for trace/debug history

use crate::storage::TaskStorage;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::warn;
use types::store::ReplySender;

use super::events::{TaskEventEmitter, TaskStreamEvent};
use super::executor::ReplySenderFactory;

/// Builds task-scoped reply senders for task execution.
pub struct TaskReplySenderFactory {
    storage: Arc<TaskStorage>,
    event_emitter: Arc<dyn TaskEventEmitter>,
}

impl TaskReplySenderFactory {
    pub fn new(storage: Arc<TaskStorage>, event_emitter: Arc<dyn TaskEventEmitter>) -> Self {
        Self {
            storage,
            event_emitter,
        }
    }
}

impl ReplySenderFactory for TaskReplySenderFactory {
    fn for_task(&self, task_id: &str, _agent_id: &str) -> Option<Arc<dyn ReplySender>> {
        Some(Arc::new(TaskReplySender {
            task_id: task_id.to_string(),
            storage: self.storage.clone(),
            event_emitter: self.event_emitter.clone(),
        }))
    }
}

struct TaskReplySender {
    task_id: String,
    storage: Arc<TaskStorage>,
    event_emitter: Arc<dyn TaskEventEmitter>,
}

impl ReplySender for TaskReplySender {
    fn send(&self, message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        let task_id = self.task_id.clone();
        let storage = self.storage.clone();
        let event_emitter = self.event_emitter.clone();

        Box::pin(async move {
            let trimmed = message.trim();
            if trimmed.is_empty() {
                return Ok(());
            }
            let content = trimmed.to_string();

            if let Err(error) = storage.log_task_reply(&task_id, content.clone()) {
                warn!(
                    task_id = %task_id,
                    error = %error,
                    "Failed to persist background reply message"
                );
            }

            let stream_output = if content.ends_with('\n') {
                content.clone()
            } else {
                format!("{content}\n")
            };
            event_emitter
                .emit(TaskStreamEvent::output(&task_id, stream_output, false))
                .await;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TaskMessageSource, TaskMessageStatus, TaskSchedule};
    use crate::runtime::task_runtime::StreamEventKind;
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct CaptureEventEmitter {
        events: Mutex<Vec<TaskStreamEvent>>,
    }

    #[async_trait]
    impl TaskEventEmitter for CaptureEventEmitter {
        async fn emit(&self, event: TaskStreamEvent) {
            self.events.lock().await.push(event);
        }
    }

    fn create_storage() -> (Arc<TaskStorage>, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("reply-sender.db");
        let db = Arc::new(redb::Database::create(db_path).expect("db"));
        let storage = Arc::new(TaskStorage::new(db).expect("storage"));
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn background_reply_sender_emits_and_persists_reply() {
        let (storage, _temp_dir) = create_storage();
        let task = storage
            .create_task(
                "Reply Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .expect("create task");

        let event_emitter = Arc::new(CaptureEventEmitter::default());
        let factory = TaskReplySenderFactory::new(storage.clone(), event_emitter.clone());
        let sender = factory.for_task(&task.id, "agent-001").expect("sender");

        sender
            .send("Received, starting now.".to_string())
            .await
            .expect("reply send");

        let events = event_emitter.events.lock().await;
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            StreamEventKind::Output { text, .. } => {
                assert!(text.contains("Received, starting now."));
            }
            other => panic!("unexpected event kind: {:?}", other),
        }
        drop(events);

        let messages = storage
            .list_task_messages(&task.id, 10)
            .expect("list messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].source, TaskMessageSource::Agent);
        assert_eq!(messages[0].status, TaskMessageStatus::Consumed);
        assert_eq!(messages[0].message, "Received, starting now.");

        let pending = storage
            .list_pending_task_messages(&task.id, 10)
            .expect("list pending");
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn background_reply_sender_succeeds_without_linked_conversation() {
        let (storage, _temp_dir) = create_storage();
        let task = storage
            .create_task(
                "No Link Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .expect("create task");

        let event_emitter = Arc::new(CaptureEventEmitter::default());
        let factory = TaskReplySenderFactory::new(storage, event_emitter);
        let sender = factory.for_task(&task.id, "agent-001").expect("sender");

        sender
            .send("working on it".to_string())
            .await
            .expect("reply send");
    }
}
