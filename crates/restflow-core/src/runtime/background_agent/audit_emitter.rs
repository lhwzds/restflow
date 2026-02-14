use async_trait::async_trait;
use restflow_ai::agent::StreamEmitter;
use std::collections::HashMap;
use std::time::Instant;

use crate::models::{AuditEntry, AuditEntryType};
use crate::storage::AuditStorage;

pub struct AuditStreamEmitter {
    task_id: String,
    execution_id: String,
    storage: AuditStorage,
    inner: Option<Box<dyn StreamEmitter>>,
    started_at: HashMap<String, (Instant, usize)>,
}

impl AuditStreamEmitter {
    pub fn new(
        task_id: String,
        execution_id: String,
        storage: AuditStorage,
        inner: Option<Box<dyn StreamEmitter>>,
    ) -> Self {
        Self {
            task_id,
            execution_id,
            storage,
            inner,
            started_at: HashMap::new(),
        }
    }

    pub fn record_execution_start(&self, agent_id: &str, model: &str, input: &str) {
        let preview = input.chars().take(300).collect::<String>();
        let entry = AuditEntry::new(
            self.task_id.clone(),
            self.execution_id.clone(),
            AuditEntryType::ExecutionStart {
                agent_id: agent_id.to_string(),
                model: model.to_string(),
                input_preview: preview,
            },
        );
        let _ = self.storage.append(&entry);
    }

    pub fn record_execution_complete(
        &self,
        total_iterations: usize,
        total_tokens: u32,
        total_cost_usd: f64,
        total_duration_ms: u64,
        success: bool,
    ) {
        let entry = AuditEntry::new(
            self.task_id.clone(),
            self.execution_id.clone(),
            AuditEntryType::ExecutionComplete {
                total_iterations,
                total_tokens,
                total_cost_usd,
                total_duration_ms,
                success,
            },
        );
        let _ = self.storage.append(&entry);
    }

    pub fn record_execution_failed(&self, error: &str, total_duration_ms: u64) {
        let entry = AuditEntry::new(
            self.task_id.clone(),
            self.execution_id.clone(),
            AuditEntryType::ExecutionFailed {
                error: error.to_string(),
                total_duration_ms,
            },
        );
        let _ = self.storage.append(&entry);
    }
}

#[async_trait]
impl StreamEmitter for AuditStreamEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        if let Some(inner) = self.inner.as_mut() {
            inner.emit_text_delta(text).await;
        }
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        if let Some(inner) = self.inner.as_mut() {
            inner.emit_thinking_delta(text).await;
        }
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.started_at
            .insert(id.to_string(), (Instant::now(), arguments.len()));
        if let Some(inner) = self.inner.as_mut() {
            inner.emit_tool_call_start(id, name, arguments).await;
        }
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        let duration_ms = self
            .started_at
            .remove(id)
            .map(|(start, input_size_bytes)| (start.elapsed().as_millis() as u64, input_size_bytes))
            .unwrap_or((0, 0));
        let entry = AuditEntry::new(
            self.task_id.clone(),
            self.execution_id.clone(),
            AuditEntryType::ToolCall {
                tool_name: name.to_string(),
                success,
                duration_ms: duration_ms.0,
                input_size_bytes: duration_ms.1,
                output_size_bytes: result.len(),
                error: if success {
                    None
                } else {
                    Some(result.to_string())
                },
                iteration: 0,
            },
        );
        let _ = self.storage.append(&entry);
        if let Some(inner) = self.inner.as_mut() {
            inner.emit_tool_call_result(id, name, result, success).await;
        }
    }

    async fn emit_llm_call(
        &mut self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: Option<f64>,
        duration_ms: u64,
        iteration: usize,
    ) {
        let entry = AuditEntry::new(
            self.task_id.clone(),
            self.execution_id.clone(),
            AuditEntryType::LlmCall {
                model: model.to_string(),
                input_tokens,
                output_tokens,
                cost_usd: cost_usd.unwrap_or(0.0),
                duration_ms,
                iteration,
            },
        );
        let _ = self.storage.append(&entry);
        if let Some(inner) = self.inner.as_mut() {
            inner
                .emit_llm_call(
                    model,
                    input_tokens,
                    output_tokens,
                    cost_usd,
                    duration_ms,
                    iteration,
                )
                .await;
        }
    }

    async fn emit_model_switch(
        &mut self,
        from_model: &str,
        to_model: &str,
        reason: &str,
        iteration: usize,
    ) {
        let entry = AuditEntry::new(
            self.task_id.clone(),
            self.execution_id.clone(),
            AuditEntryType::ModelSwitch {
                from_model: from_model.to_string(),
                to_model: to_model.to_string(),
                reason: reason.to_string(),
                iteration,
            },
        );
        let _ = self.storage.append(&entry);
        if let Some(inner) = self.inner.as_mut() {
            inner
                .emit_model_switch(from_model, to_model, reason, iteration)
                .await;
        }
    }

    async fn emit_complete(&mut self) {
        if let Some(inner) = self.inner.as_mut() {
            inner.emit_complete().await;
        }
    }
}
