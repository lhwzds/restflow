//! Reply tool — allows the agent to send intermediate messages to the user
//! during execution (before the final response).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_traits::store::ReplySender;

#[derive(Debug, Deserialize)]
struct ReplyInput {
    /// Message to send to the user.
    message: String,
}

/// Tool that lets the agent send a message to the user mid-execution.
pub struct ReplyTool {
    sender: Arc<dyn ReplySender>,
}

impl ReplyTool {
    pub fn new(sender: Arc<dyn ReplySender>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl Tool for ReplyTool {
    fn name(&self) -> &str {
        "reply"
    }

    fn description(&self) -> &str {
        "Send an intermediate message to the user during execution. Use this to acknowledge requests, provide progress updates, or share partial results before the final response. The message is delivered immediately to the user's channel (e.g., Telegram, chat)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to send to the user"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let parsed: ReplyInput = serde_json::from_value(input)
            .map_err(|e| restflow_ai::error::AiError::Tool(format!("Invalid reply input: {e}")))?;

        if parsed.message.trim().is_empty() {
            return Ok(ToolOutput::error("Message cannot be empty"));
        }

        match self.sender.send(parsed.message.clone()).await {
            Ok(()) => Ok(ToolOutput::success(
                json!({"status": "sent", "message": parsed.message}),
            )),
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to send reply: {e}. The reply channel may have closed. Check if the conversation is still active."
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockSender {
        messages: Arc<Mutex<Vec<String>>>,
    }

    struct FailingSender;

    impl ReplySender for MockSender {
        fn send(
            &self,
            message: String,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
            let messages = self.messages.clone();
            Box::pin(async move {
                messages.lock().unwrap().push(message);
                Ok(())
            })
        }
    }

    impl ReplySender for FailingSender {
        fn send(
            &self,
            _message: String,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
            Box::pin(async move { anyhow::bail!("channel closed") })
        }
    }

    #[tokio::test]
    async fn test_reply_tool_sends_message() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sender = Arc::new(MockSender {
            messages: messages.clone(),
        });
        let tool = ReplyTool::new(sender);

        let result = tool
            .execute(json!({"message": "Working on it..."}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(messages.lock().unwrap().len(), 1);
        assert_eq!(messages.lock().unwrap()[0], "Working on it...");
    }

    #[tokio::test]
    async fn test_reply_tool_rejects_empty_message() {
        let sender = Arc::new(MockSender {
            messages: Arc::new(Mutex::new(Vec::new())),
        });
        let tool = ReplyTool::new(sender);

        let result = tool.execute(json!({"message": "  "})).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_reply_tool_error_guidance() {
        let tool = ReplyTool::new(Arc::new(FailingSender));
        let result = tool.execute(json!({"message": "ping"})).await.unwrap();

        assert!(!result.success);
        assert!(
            result
                .error
                .expect("expected error")
                .contains("Check if the conversation is still active")
        );
    }
}
