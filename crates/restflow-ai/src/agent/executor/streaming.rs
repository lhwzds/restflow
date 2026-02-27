use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;

use crate::agent::ExecutionStep;
use crate::agent::stream::{ChannelEmitter, StreamEmitter, ToolCallAccumulator};
use crate::agent::streaming_buffer::{BufferMode, StreamingBuffer};
use crate::error::Result;
use crate::llm::{CompletionRequest, FinishReason};

use super::{AgentConfig, AgentExecutor};

impl AgentExecutor {
    /// Execute agent and return execution steps as an async stream.
    pub fn run_stream(
        self: Arc<Self>,
        config: AgentConfig,
    ) -> Pin<Box<dyn Stream<Item = ExecutionStep> + Send>> {
        let (tx, mut rx) = mpsc::channel::<ExecutionStep>(128);
        let executor = Arc::clone(&self);

        tokio::spawn(async move {
            let started_execution_id = uuid::Uuid::new_v4().to_string();
            if tx
                .send(ExecutionStep::Started {
                    execution_id: started_execution_id.clone(),
                })
                .await
                .is_err()
            {
                return;
            }

            let mut emitter = ChannelEmitter::new(tx.clone());
            let execution = executor.execute_with_mode(
                config,
                &mut emitter,
                true,
                Some(started_execution_id),
                None,
            );
            tokio::pin!(execution);
            let result = tokio::select! {
                result = &mut execution => result,
                _ = tx.closed() => return,
            };
            match result {
                Ok(result) => {
                    let _ = tx
                        .send(ExecutionStep::Completed {
                            result: Box::new(result),
                        })
                        .await;
                }
                Err(error) => {
                    let _ = tx
                        .send(ExecutionStep::Failed {
                            error: error.to_string(),
                        })
                        .await;
                }
            }
        });

        Box::pin(async_stream::stream! {
            while let Some(step) = rx.recv().await {
                yield step;
            }
        })
    }

    pub(crate) async fn get_streaming_completion(
        &self,
        request: CompletionRequest,
        emitter: &mut dyn StreamEmitter,
        iteration: usize,
        execution_id: &str,
        streaming_buffer: &mut StreamingBuffer,
    ) -> Result<crate::llm::CompletionResponse> {
        let _ = iteration;
        if !self.llm.supports_streaming() {
            let response = self.llm.complete(request).await?;
            if let Some(content) = &response.content
                && let Some(flushed) =
                    streaming_buffer.append(execution_id, content, BufferMode::Replace)
            {
                emitter.emit_text_delta(&flushed).await;
            }
            if let Some(flushed) = streaming_buffer.flush(execution_id) {
                emitter.emit_text_delta(&flushed).await;
            }
            return Ok(response);
        }

        let mut stream = self.llm.complete_stream(request);
        let mut text = String::new();
        let mut accumulator = ToolCallAccumulator::new();
        let mut usage = None;
        let mut finish_reason = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;

            if !chunk.text.is_empty() {
                text.push_str(&chunk.text);
                if let Some(flushed) =
                    streaming_buffer.append(execution_id, &chunk.text, BufferMode::Accumulate)
                {
                    emitter.emit_text_delta(&flushed).await;
                }
            }

            if let Some(thinking) = &chunk.thinking {
                emitter.emit_thinking_delta(thinking).await;
            }

            if let Some(delta) = &chunk.tool_call_delta {
                accumulator.accumulate(delta);
            }

            if let Some(chunk_usage) = chunk.usage {
                usage = Some(chunk_usage);
            }

            if let Some(reason) = chunk.finish_reason {
                finish_reason = Some(reason);
            }
        }

        if let Some(flushed) = streaming_buffer.flush(execution_id) {
            emitter.emit_text_delta(&flushed).await;
        }

        Ok(crate::llm::CompletionResponse {
            content: if text.is_empty() { None } else { Some(text) },
            tool_calls: accumulator.finalize(),
            finish_reason: finish_reason.unwrap_or(FinishReason::Stop),
            usage,
        })
    }
}
