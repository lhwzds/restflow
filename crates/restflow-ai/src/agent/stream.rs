use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

use crate::agent::ExecutionStep;
use crate::llm::{ToolCall, ToolCallDelta};

#[async_trait]
pub trait StreamEmitter: Send + Sync {
    async fn emit_text_delta(&mut self, text: &str);
    async fn emit_thinking_delta(&mut self, text: &str);
    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str);
    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool);
    async fn emit_llm_call(
        &mut self,
        _model: &str,
        _input_tokens: u32,
        _output_tokens: u32,
        _cost_usd: Option<f64>,
        _duration_ms: u64,
        _iteration: usize,
    ) {
    }
    async fn emit_model_switch(
        &mut self,
        _from_model: &str,
        _to_model: &str,
        _reason: &str,
        _iteration: usize,
    ) {
    }
    async fn emit_complete(&mut self);
}

pub struct NullEmitter;

#[async_trait]
impl StreamEmitter for NullEmitter {
    async fn emit_text_delta(&mut self, _text: &str) {}
    async fn emit_thinking_delta(&mut self, _text: &str) {}
    async fn emit_tool_call_start(&mut self, _id: &str, _name: &str, _arguments: &str) {}
    async fn emit_tool_call_result(
        &mut self,
        _id: &str,
        _name: &str,
        _result: &str,
        _success: bool,
    ) {
    }
    async fn emit_complete(&mut self) {}
}

pub struct ChannelEmitter {
    tx: mpsc::Sender<ExecutionStep>,
}

impl ChannelEmitter {
    pub fn new(tx: mpsc::Sender<ExecutionStep>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl StreamEmitter for ChannelEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        let _ = self
            .tx
            .send(ExecutionStep::TextDelta {
                content: text.to_string(),
            })
            .await;
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        let _ = self
            .tx
            .send(ExecutionStep::ThinkingDelta {
                content: text.to_string(),
            })
            .await;
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        let _ = self
            .tx
            .send(ExecutionStep::ToolCallStart {
                id: id.to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
            })
            .await;
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        let _ = self
            .tx
            .send(ExecutionStep::ToolCallResult {
                id: id.to_string(),
                name: name.to_string(),
                result: result.to_string(),
                success,
            })
            .await;
    }

    async fn emit_complete(&mut self) {}
}

#[derive(Debug, Clone)]
struct ToolCallBuilder {
    index: usize,
    id: String,
    name: String,
    arguments_json: String,
}

#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    builders: BTreeMap<usize, ToolCallBuilder>,
}

impl ToolCallAccumulator {
    pub fn new() -> Self {
        Self {
            builders: BTreeMap::new(),
        }
    }

    pub fn accumulate(&mut self, delta: &ToolCallDelta) {
        let builder = self
            .builders
            .entry(delta.index)
            .or_insert_with(|| ToolCallBuilder {
                index: delta.index,
                id: String::new(),
                name: String::new(),
                arguments_json: String::new(),
            });

        if let Some(id) = &delta.id
            && builder.id.is_empty()
        {
            builder.id = id.clone();
        }

        if let Some(name) = &delta.name
            && builder.name.is_empty()
        {
            builder.name = name.clone();
        }

        if let Some(args) = &delta.arguments {
            builder.arguments_json.push_str(args);
        }
    }

    pub fn finalize(self) -> Vec<ToolCall> {
        self.builders
            .into_values()
            .map(|builder| ToolCall {
                id: builder.id,
                name: builder.name,
                arguments: parse_arguments(&builder.arguments_json),
            })
            .collect()
    }
}

fn parse_arguments(json: &str) -> Value {
    if json.trim().is_empty() {
        return Value::Null;
    }
    match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                json_len = json.len(),
                error = %e,
                "Failed to parse tool call arguments, passing empty object"
            );
            Value::Object(serde_json::Map::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_tool_call_accumulator_single() {
        let mut acc = ToolCallAccumulator::new();

        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            name: Some("lookup".to_string()),
            arguments: Some("{\"id\":".to_string()),
        });
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments: Some("1}".to_string()),
        });

        let calls = acc.finalize();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "lookup");
        assert_eq!(calls[0].arguments, serde_json::json!({"id": 1}));
    }

    #[test]
    fn test_tool_call_accumulator_multiple() {
        let mut acc = ToolCallAccumulator::new();

        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            name: Some("one".to_string()),
            arguments: Some("{\"a\":".to_string()),
        });
        acc.accumulate(&ToolCallDelta {
            index: 1,
            id: Some("call_2".to_string()),
            name: Some("two".to_string()),
            arguments: Some("{\"b\":".to_string()),
        });
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments: Some("1}".to_string()),
        });
        acc.accumulate(&ToolCallDelta {
            index: 1,
            id: None,
            name: None,
            arguments: Some("2}".to_string()),
        });

        let calls = acc.finalize();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "one");
        assert_eq!(calls[1].name, "two");
    }

    #[test]
    fn test_tool_call_accumulator_empty() {
        let acc = ToolCallAccumulator::new();
        let calls = acc.finalize();
        assert!(calls.is_empty());
    }

    #[tokio::test]
    async fn test_null_emitter() {
        let mut emitter = NullEmitter;
        emitter.emit_text_delta("hello").await;
        emitter.emit_thinking_delta("think").await;
        emitter.emit_tool_call_start("id", "name", "{}").await;
        emitter
            .emit_tool_call_result("id", "name", "ok", true)
            .await;
        emitter.emit_complete().await;
    }

    #[tokio::test]
    async fn test_channel_emitter_sends_steps() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut emitter = ChannelEmitter::new(tx);

        emitter.emit_text_delta("hello").await;
        emitter.emit_thinking_delta("plan").await;
        emitter.emit_tool_call_start("call_1", "echo", "{}").await;
        emitter
            .emit_tool_call_result("call_1", "echo", "{\"ok\":true}", true)
            .await;

        let step = rx.recv().await.unwrap();
        assert!(matches!(step, ExecutionStep::TextDelta { .. }));

        let step = rx.recv().await.unwrap();
        assert!(matches!(step, ExecutionStep::ThinkingDelta { .. }));

        let step = rx.recv().await.unwrap();
        assert!(matches!(step, ExecutionStep::ToolCallStart { .. }));

        let step = rx.recv().await.unwrap();
        assert!(matches!(step, ExecutionStep::ToolCallResult { .. }));
    }
}
