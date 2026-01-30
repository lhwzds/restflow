//! Main Agent event system for frontend updates.
//!
//! This module provides real-time event streaming from the main agent
//! to the frontend, including sub-agent status updates.

use super::tracker::SubagentResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;
use ts_rs::TS;

/// Event name for Tauri event system
pub const MAIN_AGENT_EVENT: &str = "main-agent:event";

/// Main Agent event wrapper
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MainAgentEvent {
    /// Session ID
    pub session_id: String,

    /// Timestamp (Unix ms)
    pub timestamp: i64,

    /// Event kind
    pub kind: MainAgentEventKind,
}

/// Main Agent event kinds
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MainAgentEventKind {
    /// Session started
    SessionStarted { model: String },

    /// User message received
    UserMessage { content: String },

    /// Main agent is thinking
    Thinking { content: String },

    /// Tool call started
    ToolCallStart {
        tool_name: String,
        #[ts(type = "any")]
        arguments: Value,
    },

    /// Tool call completed
    ToolCallEnd {
        tool_name: String,
        #[ts(type = "any")]
        result: Value,
        success: bool,
    },

    /// Sub-agent spawned
    SubagentSpawned {
        task_id: String,
        agent_name: String,
        task: String,
    },

    /// Sub-agent progress update
    SubagentProgress {
        task_id: String,
        agent_name: String,
        step: String,
    },

    /// Sub-agent completed
    SubagentCompleted {
        task_id: String,
        agent_name: String,
        success: bool,
        summary: Option<String>,
        duration_ms: u64,
    },

    /// Skill loaded
    SkillLoaded { skill_id: String, skill_name: String },

    /// Assistant token (streaming)
    AssistantToken { text: String, token_count: u32 },

    /// Response completed
    ResponseCompleted {
        full_content: String,
        total_tokens: u32,
        duration_ms: u64,
    },

    /// Error occurred
    Error { message: String },
}

/// Trait for emitting main agent events
#[async_trait]
pub trait MainAgentEventEmitter: Send + Sync {
    /// Emit an event
    fn emit(&self, event: MainAgentEvent);

    /// Emit sub-agent started event
    fn emit_subagent_started(&self, session_id: &str, task_id: &str, agent_name: &str, task: &str) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::SubagentSpawned {
                task_id: task_id.to_string(),
                agent_name: agent_name.to_string(),
                task: task.to_string(),
            },
        });
    }

    /// Emit sub-agent progress event
    fn emit_subagent_step(&self, session_id: &str, task_id: &str, agent_name: &str, step: &str) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::SubagentProgress {
                task_id: task_id.to_string(),
                agent_name: agent_name.to_string(),
                step: step.to_string(),
            },
        });
    }

    /// Emit sub-agent completed event
    fn emit_subagent_completed(
        &self,
        session_id: &str,
        task_id: &str,
        agent_name: &str,
        result: &SubagentResult,
    ) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::SubagentCompleted {
                task_id: task_id.to_string(),
                agent_name: agent_name.to_string(),
                success: result.success,
                summary: result.summary.clone(),
                duration_ms: result.duration_ms,
            },
        });
    }

    /// Emit thinking event
    fn emit_thinking(&self, session_id: &str, content: &str) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::Thinking {
                content: content.to_string(),
            },
        });
    }

    /// Emit tool call start event
    fn emit_tool_call_start(&self, session_id: &str, tool_name: &str, arguments: Value) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::ToolCallStart {
                tool_name: tool_name.to_string(),
                arguments,
            },
        });
    }

    /// Emit tool call end event
    fn emit_tool_call_end(&self, session_id: &str, tool_name: &str, result: Value, success: bool) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::ToolCallEnd {
                tool_name: tool_name.to_string(),
                result,
                success,
            },
        });
    }

    /// Emit assistant token (for streaming)
    fn emit_token(&self, session_id: &str, text: &str, token_count: u32) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::AssistantToken {
                text: text.to_string(),
                token_count,
            },
        });
    }

    /// Emit error event
    fn emit_error(&self, session_id: &str, message: &str) {
        self.emit(MainAgentEvent {
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::Error {
                message: message.to_string(),
            },
        });
    }
}

/// Tauri event emitter implementation
pub struct TauriMainAgentEmitter {
    app_handle: tauri::AppHandle,
}

impl TauriMainAgentEmitter {
    /// Create a new Tauri emitter
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

impl MainAgentEventEmitter for TauriMainAgentEmitter {
    fn emit(&self, event: MainAgentEvent) {
        let _ = self.app_handle.emit(MAIN_AGENT_EVENT, event);
    }
}

/// No-op event emitter for testing
pub struct NoopMainAgentEmitter;

impl MainAgentEventEmitter for NoopMainAgentEmitter {
    fn emit(&self, _event: MainAgentEvent) {
        // Do nothing
    }
}

/// Channel-based event emitter for testing
pub struct ChannelMainAgentEmitter {
    tx: tokio::sync::mpsc::UnboundedSender<MainAgentEvent>,
}

impl ChannelMainAgentEmitter {
    /// Create a new channel emitter
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<MainAgentEvent>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

impl MainAgentEventEmitter for ChannelMainAgentEmitter {
    fn emit(&self, event: MainAgentEvent) {
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = MainAgentEvent {
            session_id: "session-1".to_string(),
            timestamp: 1234567890,
            kind: MainAgentEventKind::SubagentSpawned {
                task_id: "task-1".to_string(),
                agent_name: "researcher".to_string(),
                task: "Research X".to_string(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("subagent_spawned"));
        assert!(json.contains("researcher"));
    }

    #[test]
    fn test_noop_emitter() {
        let emitter = NoopMainAgentEmitter;
        emitter.emit(MainAgentEvent {
            session_id: "test".to_string(),
            timestamp: 0,
            kind: MainAgentEventKind::Error {
                message: "test".to_string(),
            },
        });
        // Should not panic
    }

    #[tokio::test]
    async fn test_channel_emitter() {
        let (emitter, mut rx) = ChannelMainAgentEmitter::new();

        emitter.emit(MainAgentEvent {
            session_id: "test".to_string(),
            timestamp: 123,
            kind: MainAgentEventKind::Thinking {
                content: "Hmm...".to_string(),
            },
        });

        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id, "test");
        if let MainAgentEventKind::Thinking { content } = received.kind {
            assert_eq!(content, "Hmm...");
        } else {
            panic!("Wrong event kind");
        }
    }
}
