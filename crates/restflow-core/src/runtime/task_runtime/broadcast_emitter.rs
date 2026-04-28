use async_trait::async_trait;
use restflow_ai::agent::StreamEmitter;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::channel::{ChannelRouter, MessageLevel};

const MAX_BROADCAST_CHARS: usize = 200;
const TYPING_INTERVAL_SECS: u64 = 5;

pub struct BroadcastStreamEmitter {
    task_name: String,
    router: Arc<ChannelRouter>,
    started_at: HashMap<String, Instant>,
    typing_task: Option<JoinHandle<()>>,
}

impl BroadcastStreamEmitter {
    pub fn new(task_name: String, router: Arc<ChannelRouter>) -> Self {
        let typing_router = Arc::clone(&router);
        let typing_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(TYPING_INTERVAL_SECS));
            loop {
                interval.tick().await;
                for (channel_type, result) in typing_router.broadcast_typing().await {
                    if let Err(error) = result {
                        warn!(
                            channel = ?channel_type,
                            error = %error,
                            "Failed to broadcast typing indicator"
                        );
                    }
                }
            }
        });

        Self {
            task_name,
            router,
            started_at: HashMap::new(),
            typing_task: Some(typing_task),
        }
    }

    fn tool_start_message(&self, tool_name: &str) -> String {
        format!("🤖 [{}] Calling {}...", self.task_name, tool_name)
    }

    fn tool_result_message(
        &self,
        tool_name: &str,
        result: &str,
        success: bool,
        elapsed_secs: f64,
    ) -> String {
        if success {
            format!(
                "🤖 [{}] ✓ {} completed ({:.1}s)",
                self.task_name, tool_name, elapsed_secs
            )
        } else {
            let truncated = truncate_text(result, MAX_BROADCAST_CHARS);
            format!(
                "🤖 [{}] ✗ {} failed ({:.1}s): {}",
                self.task_name, tool_name, elapsed_secs, truncated
            )
        }
    }

    async fn broadcast(&self, message: &str) {
        for (channel_type, result) in self.router.broadcast(message, MessageLevel::Plain).await {
            if let Err(error) = result {
                warn!(
                    channel = ?channel_type,
                    task_name = %self.task_name,
                    error = %error,
                    "Failed to broadcast step update"
                );
            }
        }
    }

    fn stop_typing_task(&mut self) {
        if let Some(handle) = self.typing_task.take() {
            handle.abort();
        }
    }
}

#[async_trait]
impl StreamEmitter for BroadcastStreamEmitter {
    async fn emit_text_delta(&mut self, _text: &str) {}

    async fn emit_thinking_delta(&mut self, _text: &str) {}

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, _arguments: &str) {
        self.started_at.insert(id.to_string(), Instant::now());
        self.broadcast(&self.tool_start_message(name)).await;
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        let elapsed_secs = self
            .started_at
            .remove(id)
            .map(|start| start.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let message = self.tool_result_message(name, result, success, elapsed_secs);
        self.broadcast(&message).await;
    }

    async fn emit_complete(&mut self) {
        self.stop_typing_task();
    }
}

impl Drop for BroadcastStreamEmitter {
    fn drop(&mut self) {
        self.stop_typing_task();
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }

    let mut truncated: String = value.chars().take(max_chars).collect();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelRouter, ChannelType, OutboundMessage};
    use anyhow::Result;
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;
    use tokio::sync::Mutex;

    #[derive(Clone)]
    struct TestChannel {
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
    }

    #[async_trait]
    impl Channel for TestChannel {
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

        fn start_receiving(
            &self,
        ) -> Option<Pin<Box<dyn Stream<Item = crate::channel::InboundMessage> + Send>>> {
            None
        }
    }

    fn build_router(sent: Arc<Mutex<Vec<OutboundMessage>>>) -> Arc<ChannelRouter> {
        let mut router = ChannelRouter::new();
        router.register_with_default(TestChannel { sent }, "chat-1");
        Arc::new(router)
    }

    #[tokio::test]
    async fn test_broadcast_stream_emitter_sends_start_and_success_messages() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let router = build_router(sent.clone());
        let mut emitter = BroadcastStreamEmitter::new("MyAgent".to_string(), router);

        emitter.emit_tool_call_start("call-1", "bash", "{}").await;
        emitter
            .emit_tool_call_result("call-1", "bash", "ok", true)
            .await;

        let messages = sent.lock().await.clone();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "🤖 [MyAgent] Calling bash...");
        assert!(
            messages[1]
                .content
                .contains("🤖 [MyAgent] ✓ bash completed (")
        );
    }

    #[tokio::test]
    async fn test_broadcast_stream_emitter_truncates_failure_output() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let router = build_router(sent.clone());
        let mut emitter = BroadcastStreamEmitter::new("MyAgent".to_string(), router);
        let long_error = "x".repeat(500);

        emitter
            .emit_tool_call_result("call-2", "http", &long_error, false)
            .await;

        let messages = sent.lock().await.clone();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].content.contains("🤖 [MyAgent] ✗ http failed"));
        assert!(messages[0].content.contains("..."));
    }

    #[test]
    fn test_truncate_text_respects_limit() {
        let input = "abcdef";
        assert_eq!(truncate_text(input, 6), "abcdef");
        assert_eq!(truncate_text(input, 3), "abc...");
    }
}
