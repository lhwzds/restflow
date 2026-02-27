//! Background-agent reply sender integration for the `reply` tool.
//!
//! This module wires execution-scoped reply semantics for background tasks:
//! - emit a live task stream output event
//! - deliver to task-linked channel conversations (when available)
//! - persist agent-originated reply messages for audit/debug history

use crate::channel::{ChannelRouter, OutboundMessage};
use crate::storage::BackgroundAgentStorage;
use anyhow::anyhow;
use restflow_traits::store::ReplySender;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::events::{TaskEventEmitter, TaskStreamEvent};
use super::executor::ReplySenderFactory;

/// Builds task-scoped reply senders for background-agent execution.
pub struct BackgroundReplySenderFactory {
    storage: Arc<BackgroundAgentStorage>,
    event_emitter: Arc<dyn TaskEventEmitter>,
    channel_router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
}

impl BackgroundReplySenderFactory {
    pub fn new(
        storage: Arc<BackgroundAgentStorage>,
        event_emitter: Arc<dyn TaskEventEmitter>,
        channel_router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
    ) -> Self {
        Self {
            storage,
            event_emitter,
            channel_router,
        }
    }
}

impl ReplySenderFactory for BackgroundReplySenderFactory {
    fn for_background_task(&self, task_id: &str, _agent_id: &str) -> Option<Arc<dyn ReplySender>> {
        Some(Arc::new(BackgroundTaskReplySender {
            task_id: task_id.to_string(),
            storage: self.storage.clone(),
            event_emitter: self.event_emitter.clone(),
            channel_router: self.channel_router.clone(),
        }))
    }
}

struct BackgroundTaskReplySender {
    task_id: String,
    storage: Arc<BackgroundAgentStorage>,
    event_emitter: Arc<dyn TaskEventEmitter>,
    channel_router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
}

impl ReplySender for BackgroundTaskReplySender {
    fn send(&self, message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        let task_id = self.task_id.clone();
        let storage = self.storage.clone();
        let event_emitter = self.event_emitter.clone();
        let channel_router = self.channel_router.clone();

        Box::pin(async move {
            let trimmed = message.trim();
            if trimmed.is_empty() {
                return Ok(());
            }
            let content = trimmed.to_string();

            if let Err(error) = storage.log_background_agent_reply(&task_id, content.clone()) {
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

            let Some(router) = channel_router.read().await.as_ref().cloned() else {
                return Ok(());
            };

            let conversations = router.find_conversations_by_task(&task_id).await;
            if conversations.is_empty() {
                debug!(
                    task_id = %task_id,
                    "No linked conversation found for background reply delivery"
                );
                return Ok(());
            }

            let mut sent_any = false;
            let mut failures = Vec::new();
            for context in conversations {
                let outbound = OutboundMessage::plain(&context.conversation_id, content.clone());
                match router.send_to(context.channel_type, outbound).await {
                    Ok(()) => sent_any = true,
                    Err(error) => failures.push(format!("{}: {}", context.conversation_id, error)),
                }
            }

            if sent_any {
                Ok(())
            } else {
                Err(anyhow!(
                    "Failed to deliver background reply: {}",
                    failures.join(" | ")
                ))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelType, InboundMessage};
    use crate::models::{BackgroundAgentSchedule, BackgroundMessageSource};
    use crate::runtime::background_agent::StreamEventKind;
    use anyhow::Result;
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;
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

    struct CaptureChannel {
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
    }

    #[async_trait]
    impl Channel for CaptureChannel {
        fn channel_type(&self) -> ChannelType {
            ChannelType::Telegram
        }

        fn is_configured(&self) -> bool {
            true
        }

        async fn send(&self, message: OutboundMessage) -> Result<()> {
            self.sent.lock().await.push(message);
            Ok(())
        }

        async fn send_typing(&self, _conversation_id: &str) -> Result<()> {
            Ok(())
        }

        fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
            None
        }
    }

    fn create_storage() -> (Arc<BackgroundAgentStorage>, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("reply-sender.db");
        let db = Arc::new(redb::Database::create(db_path).expect("db"));
        let storage = Arc::new(BackgroundAgentStorage::new(db).expect("storage"));
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn background_reply_sender_delivers_and_persists_reply() {
        let (storage, _temp_dir) = create_storage();
        let task = storage
            .create_task(
                "Reply Task".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .expect("create task");

        let sent = Arc::new(Mutex::new(Vec::<OutboundMessage>::new()));
        let mut router = ChannelRouter::new();
        router.register(CaptureChannel { sent: sent.clone() });
        let router = Arc::new(router);

        let inbound =
            InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", "hello");
        router
            .record_conversation(&inbound, Some(task.id.clone()))
            .await;

        let channel_router = Arc::new(RwLock::new(Some(router)));
        let event_emitter = Arc::new(CaptureEventEmitter::default());
        let factory = BackgroundReplySenderFactory::new(
            storage.clone(),
            event_emitter.clone(),
            channel_router,
        );
        let sender = factory
            .for_background_task(&task.id, "agent-001")
            .expect("sender");

        sender
            .send("Received, starting now.".to_string())
            .await
            .expect("reply send");

        let outgoing = sent.lock().await;
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].content, "Received, starting now.");
        drop(outgoing);

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
            .list_background_agent_messages(&task.id, 10)
            .expect("list messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].source, BackgroundMessageSource::Agent);
        assert_eq!(
            messages[0].status,
            crate::models::BackgroundMessageStatus::Consumed
        );
        assert_eq!(messages[0].message, "Received, starting now.");

        let pending = storage
            .list_pending_background_messages(&task.id, 10)
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
                BackgroundAgentSchedule::default(),
            )
            .expect("create task");

        let event_emitter = Arc::new(CaptureEventEmitter::default());
        let factory =
            BackgroundReplySenderFactory::new(storage, event_emitter, Arc::new(RwLock::new(None)));
        let sender = factory
            .for_background_task(&task.id, "agent-001")
            .expect("sender");

        sender
            .send("working on it".to_string())
            .await
            .expect("reply send");
    }
}
