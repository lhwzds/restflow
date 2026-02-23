use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Command payload for a steer message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SteerCommand {
    /// Inject a text message into the running conversation.
    Message { instruction: String },
    /// Interrupt execution and create a checkpoint.
    Interrupt {
        reason: String,
        #[serde(default)]
        metadata: Value,
    },
    /// Cancel a specific running tool call by its ID.
    CancelToolCall { tool_call_id: String },
}

/// A message injected into a running agent's ReAct loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteerMessage {
    /// The command to execute.
    pub command: SteerCommand,
    pub source: SteerSource,
    pub timestamp: i64,
}

impl SteerMessage {
    /// Create a text-injection steer message (backward-compatible helper).
    pub fn message(instruction: impl Into<String>, source: SteerSource) -> Self {
        Self {
            command: SteerCommand::Message {
                instruction: instruction.into(),
            },
            source,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Create an interrupt steer message.
    pub fn interrupt(reason: impl Into<String>, source: SteerSource) -> Self {
        Self {
            command: SteerCommand::Interrupt {
                reason: reason.into(),
                metadata: Value::Null,
            },
            source,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Backward-compatible accessor: returns the instruction text for Message
    /// commands, the reason for Interrupt commands, or the tool_call_id for
    /// CancelToolCall commands.
    pub fn instruction(&self) -> &str {
        match &self.command {
            SteerCommand::Message { instruction } => instruction,
            SteerCommand::Interrupt { reason, .. } => reason,
            SteerCommand::CancelToolCall { tool_call_id } => tool_call_id,
        }
    }

    /// Create a cancel-tool-call steer message.
    pub fn cancel_tool_call(tool_call_id: impl Into<String>, source: SteerSource) -> Self {
        Self {
            command: SteerCommand::CancelToolCall {
                tool_call_id: tool_call_id.into(),
            },
            source,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SteerSource {
    /// Direct from UI or CLI.
    User,
    /// From Telegram channel.
    Telegram,
    /// From a hook or automation.
    Hook,
    /// From REST/WebSocket API.
    Api,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steer_message_constructor() {
        let msg = SteerMessage::message("do something", SteerSource::User);
        assert_eq!(msg.instruction(), "do something");
        assert!(matches!(msg.command, SteerCommand::Message { .. }));
    }

    #[test]
    fn test_steer_interrupt_constructor() {
        let msg = SteerMessage::interrupt("approval needed", SteerSource::Api);
        assert_eq!(msg.instruction(), "approval needed");
        assert!(matches!(msg.command, SteerCommand::Interrupt { .. }));
    }

    #[test]
    fn test_steer_command_serialization() {
        let cmd = SteerCommand::Interrupt {
            reason: "test".into(),
            metadata: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("interrupt"));

        let deserialized: SteerCommand = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, SteerCommand::Interrupt { .. }));
    }

    #[test]
    fn test_steer_command_message_backward_compat() {
        let cmd = SteerCommand::Message {
            instruction: "hello".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: SteerCommand = serde_json::from_str(&json).unwrap();
        match deserialized {
            SteerCommand::Message { instruction } => assert_eq!(instruction, "hello"),
            _ => panic!("Expected Message variant"),
        }
    }
}
