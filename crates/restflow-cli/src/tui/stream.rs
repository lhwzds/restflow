use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use restflow_ai::error::Result as AiResult;
use restflow_ai::llm::{
    CompletionRequest, FinishReason, LlmClient, Message, StreamChunk, ToolCall, ToolCallDelta,
    TokenUsage,
};
use restflow_ai::memory::WorkingMemory;
use restflow_ai::tools::ToolRegistry;
use restflow_ai::{AgentConfig, AgentState};

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    Thinking(String),
    ToolStart { name: String, input: String },
    ToolEnd {
        name: String,
        output: String,
        success: bool,
    },
    TokenUpdate {
        input_tokens: u32,
        output_tokens: u32,
    },
    Complete {
        response: String,
        total_tokens: u32,
        duration_ms: u64,
    },
    Error(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct StreamCancelHandle {
    sender: broadcast::Sender<()>,
}

impl StreamCancelHandle {
    pub fn new() -> (Self, StreamCancelReceiver) {
        let (sender, receiver) = broadcast::channel(1);
        (Self { sender }, StreamCancelReceiver { receiver })
    }

    pub fn cancel(&self) {
        let _ = self.sender.send(());
    }
}

#[derive(Debug)]
pub struct StreamCancelReceiver {
    receiver: broadcast::Receiver<()>,
}

impl StreamCancelReceiver {
    pub fn is_cancelled(&mut self) -> bool {
        self.receiver.try_recv().is_ok()
    }
}

#[derive(Debug, Clone)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCallBuilder {
    fn new() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        }
    }

    fn apply_delta(&mut self, delta: ToolCallDelta) {
        if let Some(id) = delta.id {
            self.id.push_str(&id);
        }
        if let Some(name) = delta.name {
            self.name.push_str(&name);
        }
        if let Some(args) = delta.arguments {
            self.arguments.push_str(&args);
        }
    }

    fn finalize(&self) -> ToolCall {
        let args = serde_json::from_str(&self.arguments)
            .unwrap_or_else(|_| Value::String(self.arguments.clone()));
        ToolCall {
            id: if self.id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                self.id.clone()
            },
            name: if self.name.is_empty() {
                "unknown".to_string()
            } else {
                self.name.clone()
            },
            arguments: args,
        }
    }
}

pub struct StreamingExecutor {
    llm: Arc<dyn LlmClient>,
    tools: Arc<ToolRegistry>,
    event_tx: mpsc::UnboundedSender<StreamEvent>,
}

struct StreamOutcome {
    response: restflow_ai::llm::CompletionResponse,
    cancelled: bool,
}

impl StreamingExecutor {
    pub fn new(
        llm: Arc<dyn LlmClient>,
        tools: Arc<ToolRegistry>,
        event_tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> Self {
        Self {
            llm,
            tools,
            event_tx,
        }
    }

    pub async fn execute(
        &self,
        mut state: AgentState,
        mut memory: WorkingMemory,
        config: AgentConfig,
        mut cancel: StreamCancelReceiver,
    ) -> AiResult<()> {
        let start = Instant::now();
        let mut total_tokens: u32 = 0;

        let user_message = Message::user(&config.goal);
        state.add_message(user_message.clone());
        memory.add(user_message);

        while state.iteration < state.max_iterations && !state.is_terminal() {
            if cancel.is_cancelled() {
                let _ = self.event_tx.send(StreamEvent::Cancelled);
                return Ok(());
            }

            let mut request =
                CompletionRequest::new(memory.get_messages()).with_tools(self.tools.schemas());
            if let Some(temp) = config.temperature {
                request = request.with_temperature(temp);
            }

            let response = if self.llm.supports_streaming() {
                let outcome = self.stream_request(request, &mut cancel).await?;
                if outcome.cancelled {
                    let _ = self.event_tx.send(StreamEvent::Cancelled);
                    return Ok(());
                }
                outcome.response
            } else {
                self.llm.complete(request).await?
            };

            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens;
                let _ = self.event_tx.send(StreamEvent::TokenUpdate {
                    input_tokens: usage.prompt_tokens,
                    output_tokens: usage.completion_tokens,
                });
            }

            if response.tool_calls.is_empty() {
                let answer = response.content.unwrap_or_default();
                let _ = self.event_tx.send(StreamEvent::Complete {
                    response: answer,
                    total_tokens,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(());
            }

            let tool_call_msg = Message::assistant_with_tool_calls(
                response.content.clone(),
                response.tool_calls.clone(),
            );
            state.add_message(tool_call_msg.clone());
            memory.add(tool_call_msg);

            for tool_call in &response.tool_calls {
                let input_pretty = serde_json::to_string_pretty(&tool_call.arguments)
                    .unwrap_or_else(|_| tool_call.arguments.to_string());
                let _ = self.event_tx.send(StreamEvent::ToolStart {
                    name: tool_call.name.clone(),
                    input: input_pretty,
                });

                let result = tokio::time::timeout(
                    config.tool_timeout,
                    self.tools.execute(&tool_call.name, tool_call.arguments.clone()),
                )
                .await;

                let (output, success) = match result {
                    Ok(Ok(output)) => {
                        let result_text = output.result.to_string();
                        (result_text, output.success)
                    }
                    Ok(Err(err)) => (err.to_string(), false),
                    Err(_) => (
                        format!("Tool {} timed out", tool_call.name),
                        false,
                    ),
                };

                let _ = self.event_tx.send(StreamEvent::ToolEnd {
                    name: tool_call.name.clone(),
                    output: output.clone(),
                    success,
                });

                let tool_result_msg = Message::tool_result(tool_call.id.clone(), output);
                state.add_message(tool_result_msg.clone());
                memory.add(tool_result_msg);
            }

            state.increment_iteration();
        }

        let _ = self.event_tx.send(StreamEvent::Error(
            "Max iterations reached".to_string(),
        ));
        Ok(())
    }

    async fn stream_request(
        &self,
        request: CompletionRequest,
        cancel: &mut StreamCancelReceiver,
    ) -> AiResult<StreamOutcome> {
        let mut stream = self.llm.complete_stream(request);
        let mut text = String::new();
        let mut builders: Vec<ToolCallBuilder> = Vec::new();
        let mut finish_reason = FinishReason::Stop;
        let mut usage: Option<TokenUsage> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if cancel.is_cancelled() {
                return Ok(StreamOutcome {
                    response: restflow_ai::llm::CompletionResponse {
                        content: if text.is_empty() { None } else { Some(text) },
                        tool_calls: Vec::new(),
                        finish_reason: FinishReason::Stop,
                        usage: usage.clone(),
                    },
                    cancelled: true,
                });
            }

            Self::handle_chunk(
                &chunk,
                &mut text,
                &mut builders,
                &mut finish_reason,
                &mut usage,
                &self.event_tx,
            );
        }

        let tool_calls = builders.iter().map(|b| b.finalize()).collect();
        Ok(StreamOutcome {
            response: restflow_ai::llm::CompletionResponse {
                content: if text.is_empty() { None } else { Some(text) },
                tool_calls,
                finish_reason,
                usage: usage.clone(),
            },
            cancelled: false,
        })
    }

    fn handle_chunk(
        chunk: &StreamChunk,
        text: &mut String,
        builders: &mut Vec<ToolCallBuilder>,
        finish_reason: &mut FinishReason,
        usage: &mut Option<TokenUsage>,
        event_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) {
        if !chunk.text.is_empty() {
            text.push_str(&chunk.text);
            let _ = event_tx.send(StreamEvent::TextDelta(chunk.text.clone()));
        }

        if let Some(thinking) = &chunk.thinking {
            let _ = event_tx.send(StreamEvent::Thinking(thinking.clone()));
        }

        if let Some(delta) = &chunk.tool_call_delta {
            let index = delta.index;
            if builders.len() <= index {
                builders.resize_with(index + 1, ToolCallBuilder::new);
            }
            builders[index].apply_delta(delta.clone());
        }

        if let Some(reason) = chunk.finish_reason.clone() {
            *finish_reason = reason;
        }

        if let Some(chunk_usage) = &chunk.usage {
            *usage = Some(chunk_usage.clone());
        }
    }
}

pub fn build_system_prompt(config: &AgentConfig, tools: &ToolRegistry) -> String {
    let tools_desc: Vec<String> = tools
        .list()
        .iter()
        .filter_map(|name| tools.get(name))
        .map(|t| format!("- {}: {}", t.name(), t.description()))
        .collect();

    let base = config
        .system_prompt
        .as_deref()
        .unwrap_or("You are a helpful AI assistant that can use tools to accomplish tasks.");

    format!("{}\n\nAvailable tools:\n{}", base, tools_desc.join("\n"))
}

pub fn create_working_memory(
    system_prompt: &str,
    history: &[Message],
    max_messages: usize,
) -> WorkingMemory {
    let mut memory = WorkingMemory::new(max_messages);
    memory.add(Message::system(system_prompt));
    for msg in history {
        memory.add(msg.clone());
    }
    memory
}

pub fn format_tool_output(output: &str) -> String {
    if output.len() > 4000 {
        let mut trimmed = output.to_string();
        trimmed.truncate(4000);
        trimmed.push_str("... (truncated)");
        trimmed
    } else {
        output.to_string()
    }
}
