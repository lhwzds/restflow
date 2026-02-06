use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::llm::{ToolCall, ToolCallDelta};

#[async_trait]
pub trait StreamEmitter: Send + Sync {
    async fn emit_text_delta(&mut self, text: &str);
    async fn emit_thinking_delta(&mut self, text: &str);
    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str);
    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool);
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
    serde_json::from_str(json).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
