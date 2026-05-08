use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ai::llm::{AnthropicClient, LlmClient, LlmClientFactory, OpenAIClient};
use ai::tools::Tool;
use anyhow::Result;
use async_stream::stream;
use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::{Method, StatusCode, header};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{Mutex, broadcast};
use tokio::time::sleep;
use types::ClientKind;
use types::llm::LlmProvider;
use types::{ToolErrorCategory, ToolOutput};

use super::{ProviderFamily, StreamMode, StressLevel};

#[derive(Debug, Clone, Default)]
pub struct BackendMetricsSnapshot {
    pub request_count: usize,
    pub stream_requests: usize,
    pub stream_chunks: usize,
    pub max_inflight: usize,
    pub status_2xx: usize,
    pub status_429: usize,
    pub status_500: usize,
}

#[derive(Default)]
struct BackendMetrics {
    request_count: AtomicUsize,
    stream_requests: AtomicUsize,
    stream_chunks: AtomicUsize,
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    status_2xx: AtomicUsize,
    status_429: AtomicUsize,
    status_500: AtomicUsize,
}

impl BackendMetrics {
    fn begin_request(&self) -> InflightGuard<'_> {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        let current = self.inflight.fetch_add(1, Ordering::Relaxed) + 1;
        loop {
            let max = self.max_inflight.load(Ordering::Relaxed);
            if current <= max {
                break;
            }
            if self
                .max_inflight
                .compare_exchange(max, current, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        InflightGuard { metrics: self }
    }

    fn snapshot(&self) -> BackendMetricsSnapshot {
        BackendMetricsSnapshot {
            request_count: self.request_count.load(Ordering::Relaxed),
            stream_requests: self.stream_requests.load(Ordering::Relaxed),
            stream_chunks: self.stream_chunks.load(Ordering::Relaxed),
            max_inflight: self.max_inflight.load(Ordering::Relaxed),
            status_2xx: self.status_2xx.load(Ordering::Relaxed),
            status_429: self.status_429.load(Ordering::Relaxed),
            status_500: self.status_500.load(Ordering::Relaxed),
        }
    }
}

struct InflightGuard<'a> {
    metrics: &'a BackendMetrics,
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        self.metrics.inflight.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct MockLlmHttpServer {
    base_url: String,
    metrics: Arc<BackendMetrics>,
    shutdown_tx: broadcast::Sender<()>,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<Result<()>>>>>,
}

#[derive(Clone)]
pub struct StubHttpLlmFactory {
    base_url: String,
    models: HashMap<String, (ProviderFamily, StreamMode)>,
}

impl StubHttpLlmFactory {
    pub fn new(
        base_url: String,
        models: impl IntoIterator<Item = (String, (ProviderFamily, StreamMode))>,
    ) -> Self {
        Self {
            base_url,
            models: models.into_iter().collect(),
        }
    }

    fn resolve_profile(&self, model: &str) -> Option<(ProviderFamily, StreamMode)> {
        let normalized = model.trim().to_lowercase();
        if let Some(profile) = self
            .models
            .iter()
            .find(|(name, _profile)| name.to_lowercase() == normalized)
            .map(|(_, profile)| *profile)
        {
            return Some(profile);
        }

        let model_id = runtime::ModelId::from_api_name(model)
            .or_else(|| runtime::ModelId::from_serialized_str(model))
            .or_else(|| runtime::ModelId::from_canonical_id(model))?;
        let provider = match model_id.provider() {
            runtime::Provider::OpenAI => ProviderFamily::OpenAI,
            runtime::Provider::Anthropic => ProviderFamily::Anthropic,
            runtime::Provider::MiniMax | runtime::Provider::MiniMaxCodingPlan => {
                ProviderFamily::MiniMaxShim
            }
            runtime::Provider::Google => ProviderFamily::Gemini,
            _ => ProviderFamily::OpenAI,
        };
        let stream_mode = match provider {
            ProviderFamily::MiniMaxShim => StreamMode::CoarseStreaming,
            _ if model_id.is_cli_model() => StreamMode::NonStreaming,
            _ => StreamMode::Streaming,
        };
        Some((provider, stream_mode))
    }
}

impl LlmClientFactory for StubHttpLlmFactory {
    fn create_client(&self, model: &str, _api_key: Option<&str>) -> ai::Result<Arc<dyn LlmClient>> {
        let (provider, _) = self
            .resolve_profile(model)
            .ok_or_else(|| ai::AiError::Llm(format!("Unknown test model '{model}'")))?;
        let client: Arc<dyn LlmClient> = match provider {
            ProviderFamily::Anthropic | ProviderFamily::MiniMaxShim => Arc::new(
                AnthropicClient::new("stress-anthropic-key")
                    .map_err(ai::AiError::Http)?
                    .with_model(model)
                    .with_base_url(self.base_url.clone()),
            ),
            _ => Arc::new(
                OpenAIClient::new("stress-openai-key")
                    .map_err(ai::AiError::Http)?
                    .with_model(model)
                    .with_base_url(format!("{}/v1", self.base_url)),
            ),
        };
        Ok(client)
    }

    fn available_models(&self) -> Vec<String> {
        let mut models = self.models.keys().cloned().collect::<Vec<_>>();
        models.sort();
        models
    }

    fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
        None
    }

    fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
        self.resolve_profile(model)
            .map(|(provider, _)| match provider {
                ProviderFamily::OpenAI => LlmProvider::OpenAI,
                ProviderFamily::Anthropic => LlmProvider::Anthropic,
                ProviderFamily::MiniMaxShim => LlmProvider::MiniMaxCodingPlan,
                ProviderFamily::Gemini => LlmProvider::Google,
                ProviderFamily::CodexCli => LlmProvider::OpenAI,
            })
    }

    fn client_kind_for_model(&self, _model: &str) -> Option<ClientKind> {
        Some(ClientKind::Http)
    }
}

#[derive(Clone)]
struct MockLlmState {
    metrics: Arc<BackendMetrics>,
    chunk_delay: Duration,
}

#[derive(Deserialize)]
struct OpenAiStubRequest {
    model: String,
    #[serde(default)]
    stream: bool,
    messages: Vec<StubMessage>,
}

#[derive(Deserialize)]
struct AnthropicStubRequest {
    model: String,
    #[serde(default)]
    stream: bool,
    messages: Vec<StubAnthropicMessage>,
}

#[derive(Deserialize)]
struct StubMessage {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StubAnthropicContent {
    Text(String),
    Blocks(Vec<StubAnthropicContentBlock>),
}

#[derive(Deserialize)]
struct StubAnthropicContentBlock {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct StubAnthropicMessage {
    #[serde(default)]
    role: String,
    content: StubAnthropicContent,
}

impl MockLlmHttpServer {
    pub async fn start(level: StressLevel) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock llm server");
        let addr = listener.local_addr().expect("mock llm local addr");
        drop(listener);

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let metrics = Arc::new(BackendMetrics::default());
        let state = MockLlmState {
            metrics: metrics.clone(),
            chunk_delay: match level {
                StressLevel::Smoke => Duration::from_millis(2),
                StressLevel::Stress => Duration::from_millis(5),
                StressLevel::Soak => Duration::from_millis(8),
            },
        };

        let app = Router::new()
            .route("/v1/chat/completions", post(openai_chat_completions))
            .route("/v1/messages", post(anthropic_messages))
            .with_state(state);

        let task = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let mut rx = shutdown_rx.resubscribe();
                    let _ = rx.recv().await;
                })
                .await?;
            Ok(())
        });

        let server = Self {
            base_url: format!("http://{}", addr),
            metrics,
            shutdown_tx,
            task: Arc::new(Mutex::new(Some(task))),
        };

        let client = Client::new();
        for _ in 0..40 {
            if client
                .post(format!("{}/v1/chat/completions", server.base_url))
                .json(&json!({"model":"gpt-5","messages":[{"role":"user","content":"ping"}]}))
                .send()
                .await
                .is_ok()
            {
                break;
            }
            sleep(Duration::from_millis(25)).await;
        }

        server
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn metrics(&self) -> BackendMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
        if let Some(task) = self.task.lock().await.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
        }
    }
}

fn last_user_message(messages: &[StubMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(|message| message.content.clone())
        .unwrap_or_else(|| "empty-input".to_string())
}

fn has_openai_tool_result(messages: &[StubMessage]) -> bool {
    messages.iter().any(|message| message.role == "tool")
}

fn last_anthropic_user_message(messages: &[StubAnthropicMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| match &message.content {
            StubAnthropicContent::Text(text) => text.clone(),
            StubAnthropicContent::Blocks(blocks) => blocks
                .iter()
                .find_map(|block| block.text.clone())
                .unwrap_or_else(|| "empty-input".to_string()),
        })
        .unwrap_or_else(|| "empty-input".to_string())
}

fn has_anthropic_tool_result(messages: &[StubAnthropicMessage]) -> bool {
    messages.iter().any(|message| match &message.content {
        StubAnthropicContent::Blocks(blocks) => {
            blocks.iter().any(|block| block.r#type == "tool_result")
        }
        StubAnthropicContent::Text(_) => false,
    })
}

fn extract_tool_url(input: &str) -> Option<String> {
    input.split_whitespace().find_map(|token| {
        token
            .strip_prefix("tool_url=")
            .map(|value| value.to_string())
    })
}

fn extract_param(input: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    input
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&prefix).map(|value| value.to_string()))
}

fn extract_usize_param(input: &str, key: &str, default: usize) -> usize {
    extract_param(input, key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn build_large_content(label: &str, words: usize) -> String {
    (0..words)
        .map(|index| format!("{label}-{index}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn openai_tool_results_seen(messages: &[StubMessage]) -> usize {
    messages
        .iter()
        .filter(|message| message.role == "tool")
        .count()
}

fn anthropic_tool_results_seen(messages: &[StubAnthropicMessage]) -> usize {
    messages
        .iter()
        .filter(|message| match &message.content {
            StubAnthropicContent::Blocks(blocks) => {
                blocks.iter().any(|block| block.r#type == "tool_result")
            }
            StubAnthropicContent::Text(_) => false,
        })
        .count()
}

fn build_tool_call_args(step: usize, user_input: &str) -> (String, Value) {
    let workdir = extract_param(user_input, "tool_workdir").unwrap_or_else(|| ".".to_string());
    let file_path =
        extract_param(user_input, "tool_file_path").unwrap_or_else(|| format!("{workdir}/io.txt"));
    let url = extract_tool_url(user_input).unwrap_or_default();

    match step % 4 {
        0 => (
            "http_request".to_string(),
            json!({
                "method": "GET",
                "url": url,
            }),
        ),
        1 => (
            "bash".to_string(),
            json!({
                "command": format!(
                    "for i in $(seq 1 128); do printf 'bash-chunk-%s ' \"$i\"; done >> '{}' && wc -w '{}'",
                    file_path, file_path
                ),
                "workdir": workdir,
                "timeout": 30,
            }),
        ),
        2 => (
            "file".to_string(),
            json!({
                "action": "write",
                "path": file_path,
                "content": build_large_content("file-chunk", 128),
                "append": true,
            }),
        ),
        _ => (
            "file".to_string(),
            json!({
                "action": "write",
                "path": file_path,
                "content": build_large_content("skill-chunk", 128),
                "append": true,
            }),
        ),
    }
}

fn build_tool_batch(start: usize, total: usize, user_input: &str) -> Vec<(usize, String, Value)> {
    let batch_size = (total - start).min(2);
    (0..batch_size)
        .map(|offset| {
            let step = start + offset;
            let (name, args) = build_tool_call_args(step, user_input);
            (step, name, args)
        })
        .collect()
}

fn provider_style_for_model(model: &str) -> ProviderFamily {
    if let Some(model_id) = runtime::ModelId::from_api_name(model)
        .or_else(|| runtime::ModelId::from_serialized_str(model))
    {
        match model_id.provider() {
            runtime::Provider::Anthropic => ProviderFamily::Anthropic,
            runtime::Provider::Google => ProviderFamily::Gemini,
            runtime::Provider::MiniMax | runtime::Provider::MiniMaxCodingPlan => {
                ProviderFamily::MiniMaxShim
            }
            _ => ProviderFamily::OpenAI,
        }
    } else {
        ProviderFamily::OpenAI
    }
}

fn stream_mode_for_model(model: &str) -> StreamMode {
    match provider_style_for_model(model) {
        ProviderFamily::MiniMaxShim => StreamMode::CoarseStreaming,
        ProviderFamily::CodexCli => StreamMode::NonStreaming,
        _ => StreamMode::Streaming,
    }
}

async fn openai_chat_completions(
    State(state): State<MockLlmState>,
    Json(request): Json<OpenAiStubRequest>,
) -> Response {
    let _guard = state.metrics.begin_request();
    let user_input = last_user_message(&request.messages);
    let tool_url = extract_tool_url(&user_input);
    let tool_results_seen = openai_tool_results_seen(&request.messages);
    let tool_steps = extract_usize_param(&user_input, "tool_steps", 1);
    let payload_words = extract_usize_param(&user_input, "payload_words", 64);
    let content = build_large_content(
        &format!("openai:{} {}", request.model, user_input),
        payload_words,
    );
    let usage = json!({
        "prompt_tokens": 8,
        "completion_tokens": content.len(),
        "total_tokens": content.len() + 8
    });

    let should_emit_tool_call = tool_url.is_some() && tool_results_seen < tool_steps;

    if request.stream {
        state
            .metrics
            .stream_requests
            .fetch_add(1, Ordering::Relaxed);
        let mode = stream_mode_for_model(&request.model);
        let chunk_delay = state.chunk_delay;
        let metrics = state.metrics.clone();
        let stream = stream! {
            if should_emit_tool_call {
                let tool_batch = build_tool_batch(tool_results_seen, tool_steps, &user_input);
                metrics.stream_chunks.fetch_add(1, Ordering::Relaxed);
                sleep(chunk_delay).await;
                let payload = json!({
                    "choices": [{
                        "delta": {
                            "tool_calls": tool_batch.iter().enumerate().map(|(index, (step, tool_name, arguments))| json!({
                                "index": index,
                                "id": format!("tool-{step}"),
                                "function": {
                                    "name": tool_name,
                                    "arguments": arguments.to_string()
                                }
                            })).collect::<Vec<_>>()
                        },
                        "finish_reason": null
                    }]
                });
                yield Ok::<Event, std::convert::Infallible>(Event::default().data(payload.to_string()));
                let final_payload = json!({
                    "choices": [{
                        "delta": {},
                        "finish_reason": "tool_calls"
                    }],
                    "usage": usage
                });
                yield Ok::<Event, std::convert::Infallible>(Event::default().data(final_payload.to_string()));
            } else {
                let chunks = match mode {
                    StreamMode::CoarseStreaming => vec![content.clone()],
                    StreamMode::NonStreaming => vec![content.clone()],
                    StreamMode::Streaming => content
                        .split_whitespace()
                        .map(|part| format!("{part} "))
                        .collect::<Vec<_>>(),
                };
                for chunk in chunks {
                    metrics.stream_chunks.fetch_add(1, Ordering::Relaxed);
                    sleep(chunk_delay).await;
                    let payload = json!({
                        "choices": [{
                            "delta": { "content": chunk },
                            "finish_reason": null
                        }]
                    });
                    yield Ok::<Event, std::convert::Infallible>(Event::default().data(payload.to_string()));
                }
                let final_payload = json!({
                    "choices": [{
                        "delta": {},
                        "finish_reason": "stop"
                    }],
                    "usage": usage
                });
                yield Ok::<Event, std::convert::Infallible>(Event::default().data(final_payload.to_string()));
            }
            yield Ok::<Event, std::convert::Infallible>(Event::default().data("[DONE]"));
        };
        Sse::new(stream).into_response()
    } else {
        state.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
        if should_emit_tool_call {
            let tool_batch = build_tool_batch(tool_results_seen, tool_steps, &user_input);
            Json(json!({
                "choices": [{
                    "message": {
                        "content": null,
                        "tool_calls": tool_batch.iter().map(|(step, tool_name, arguments)| json!({
                            "id": format!("tool-{step}"),
                            "function": {
                                "name": tool_name,
                                "arguments": arguments.to_string()
                            }
                        })).collect::<Vec<_>>()
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": usage
            }))
            .into_response()
        } else {
            Json(json!({
                "choices": [{
                    "message": { "content": content },
                    "finish_reason": "stop"
                }],
                "usage": usage
            }))
            .into_response()
        }
    }
}

async fn anthropic_messages(
    State(state): State<MockLlmState>,
    Json(request): Json<AnthropicStubRequest>,
) -> Response {
    let _guard = state.metrics.begin_request();
    let user_input = last_anthropic_user_message(&request.messages);
    let tool_url = extract_tool_url(&user_input);
    let tool_results_seen = anthropic_tool_results_seen(&request.messages);
    let tool_steps = extract_usize_param(&user_input, "tool_steps", 1);
    let payload_words = extract_usize_param(&user_input, "payload_words", 64);
    let content = build_large_content(
        &format!("anthropic:{} {}", request.model, user_input),
        payload_words,
    );
    let should_emit_tool_call = tool_url.is_some() && tool_results_seen < tool_steps;

    if request.stream {
        state
            .metrics
            .stream_requests
            .fetch_add(1, Ordering::Relaxed);
        let mode = stream_mode_for_model(&request.model);
        let chunk_delay = state.chunk_delay;
        let metrics = state.metrics.clone();
        let stream = stream! {
            let start = json!({
                "type": "message_start",
                "message": { "usage": { "input_tokens": 8 } }
            });
            yield Ok::<Event, std::convert::Infallible>(Event::default().data(start.to_string()));

            let block_start = if should_emit_tool_call {
                Value::Null
            } else {
                json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": { "type": "text", "text": "" }
                })
            };
            if should_emit_tool_call {
                let tool_batch = build_tool_batch(tool_results_seen, tool_steps, &user_input);
                for (index, (step, tool_name, args)) in tool_batch.into_iter().enumerate() {
                    let block_start = json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {
                            "type": "tool_use",
                            "id": format!("tool-{step}"),
                            "name": tool_name
                        }
                    });
                    yield Ok::<Event, std::convert::Infallible>(Event::default().data(block_start.to_string()));

                    metrics.stream_chunks.fetch_add(1, Ordering::Relaxed);
                    sleep(chunk_delay).await;
                    let delta = json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": args.to_string()
                        }
                    });
                    yield Ok::<Event, std::convert::Infallible>(Event::default().data(delta.to_string()));

                    let stop_block = json!({
                        "type": "content_block_stop",
                        "index": index
                    });
                    yield Ok::<Event, std::convert::Infallible>(Event::default().data(stop_block.to_string()));
                }
            } else {
                yield Ok::<Event, std::convert::Infallible>(Event::default().data(block_start.to_string()));
            }

            if !should_emit_tool_call {
                let chunks = match mode {
                    StreamMode::CoarseStreaming => vec![content.clone()],
                    StreamMode::NonStreaming => vec![content.clone()],
                    StreamMode::Streaming => content
                        .split_whitespace()
                        .map(|part| format!("{part} "))
                        .collect::<Vec<_>>(),
                };
                for chunk in chunks {
                    metrics.stream_chunks.fetch_add(1, Ordering::Relaxed);
                    sleep(chunk_delay).await;
                    let delta = json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "text_delta", "text": chunk }
                    });
                    yield Ok::<Event, std::convert::Infallible>(Event::default().data(delta.to_string()));
                }
            }

            if !should_emit_tool_call {
                let stop_block = json!({
                    "type": "content_block_stop",
                    "index": 0
                });
                yield Ok::<Event, std::convert::Infallible>(Event::default().data(stop_block.to_string()));
            }

            let message_delta = json!({
                "type": "message_delta",
                "delta": { "stop_reason": if should_emit_tool_call { "tool_use" } else { "end_turn" } },
                "usage": { "output_tokens": content.len() }
            });
            yield Ok::<Event, std::convert::Infallible>(Event::default().data(message_delta.to_string()));

            let message_stop = json!({ "type": "message_stop" });
            yield Ok::<Event, std::convert::Infallible>(Event::default().data(message_stop.to_string()));
        };
        Sse::new(stream).into_response()
    } else {
        state.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
        if should_emit_tool_call {
            let tool_batch = build_tool_batch(tool_results_seen, tool_steps, &user_input);
            Json(json!({
                "content": tool_batch.iter().map(|(step, tool_name, args)| json!({
                    "type": "tool_use",
                    "id": format!("tool-{step}"),
                    "name": tool_name,
                    "input": args
                })).collect::<Vec<_>>(),
                "stop_reason": "tool_use",
                "usage": {
                    "input_tokens": 8,
                    "output_tokens": 24
                }
            }))
            .into_response()
        } else {
            Json(json!({
                "content": [{ "type": "text", "text": content }],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 8,
                    "output_tokens": content.len()
                }
            }))
            .into_response()
        }
    }
}

#[derive(Clone)]
pub struct MockToolHttpServer {
    base_url: String,
    metrics: Arc<BackendMetrics>,
    path_counts: Arc<Mutex<HashMap<String, usize>>>,
    shutdown_tx: broadcast::Sender<()>,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<Result<()>>>>>,
}

#[derive(Clone)]
struct MockToolState {
    metrics: Arc<BackendMetrics>,
    path_counts: Arc<Mutex<HashMap<String, usize>>>,
}

impl MockToolHttpServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock tool server");
        let addr = listener.local_addr().expect("mock tool local addr");
        drop(listener);

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let metrics = Arc::new(BackendMetrics::default());
        let path_counts = Arc::new(Mutex::new(HashMap::new()));
        let state = MockToolState {
            metrics: metrics.clone(),
            path_counts: path_counts.clone(),
        };

        let app = Router::new()
            .route("/ok/{id}", any(tool_ok))
            .route("/slow/{id}", any(tool_slow))
            .route("/retry/{id}", any(tool_retry))
            .route("/fatal/{id}", any(tool_fatal))
            .route("/large/{id}", any(tool_large))
            .with_state(state);

        let task = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let mut rx = shutdown_rx.resubscribe();
                    let _ = rx.recv().await;
                })
                .await?;
            Ok(())
        });

        Self {
            base_url: format!("http://{}", addr),
            metrics,
            path_counts,
            shutdown_tx,
            task: Arc::new(Mutex::new(Some(task))),
        }
    }

    pub fn url_for_round(&self, round: usize) -> String {
        let path = match round % 5 {
            0 => "ok",
            1 => "slow",
            2 => "retry",
            3 => "large",
            _ => "fatal",
        };
        format!("{}/{}/{}", self.base_url, path, round)
    }

    pub fn stable_url_for_round(&self, round: usize) -> String {
        let path = match round % 3 {
            0 => "ok",
            1 => "slow",
            _ => "large",
        };
        format!("{}/{}/{}", self.base_url, path, round)
    }

    pub fn metrics(&self) -> BackendMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub async fn path_hits(&self, key: &str) -> usize {
        self.path_counts
            .lock()
            .await
            .get(key)
            .copied()
            .unwrap_or_default()
    }

    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
        if let Some(task) = self.task.lock().await.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
        }
    }
}

async fn record_path(state: &MockToolState, key: &str) -> usize {
    let mut counts = state.path_counts.lock().await;
    let entry = counts.entry(key.to_string()).or_insert(0);
    *entry += 1;
    *entry
}

async fn tool_ok(
    State(state): State<MockToolState>,
    Path(id): Path<String>,
    method: Method,
) -> Response {
    let _guard = state.metrics.begin_request();
    let _ = record_path(&state, &format!("ok:{id}")).await;
    state.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
    Json(json!({ "ok": true, "id": id, "method": method.as_str() })).into_response()
}

async fn tool_slow(
    State(state): State<MockToolState>,
    Path(id): Path<String>,
    method: Method,
) -> Response {
    let _guard = state.metrics.begin_request();
    let _ = record_path(&state, &format!("slow:{id}")).await;
    sleep(Duration::from_millis(25)).await;
    state.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
    Json(json!({ "ok": true, "id": id, "slow": true, "method": method.as_str() })).into_response()
}

async fn tool_retry(State(state): State<MockToolState>, Path(id): Path<String>) -> Response {
    let _guard = state.metrics.begin_request();
    let hit = record_path(&state, &format!("retry:{id}")).await;
    if hit == 1 {
        state.metrics.status_429.fetch_add(1, Ordering::Relaxed);
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(json!({ "ok": false, "retry": true, "id": id })),
        )
            .into_response()
    } else {
        state.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
        Json(json!({ "ok": true, "retry": false, "id": id })).into_response()
    }
}

async fn tool_fatal(State(state): State<MockToolState>, Path(id): Path<String>) -> Response {
    let _guard = state.metrics.begin_request();
    let _ = record_path(&state, &format!("fatal:{id}")).await;
    state.metrics.status_500.fetch_add(1, Ordering::Relaxed);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "ok": false, "id": id })),
    )
        .into_response()
}

async fn tool_large(State(state): State<MockToolState>, Path(id): Path<String>) -> Response {
    let _guard = state.metrics.begin_request();
    let _ = record_path(&state, &format!("large:{id}")).await;
    state.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
    Json(json!({
        "ok": true,
        "id": id,
        "payload": "large-response-".repeat(128)
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct HttpToolInput {
    method: String,
    url: String,
    #[allow(dead_code)]
    headers: Option<Value>,
    body: Option<Value>,
}

pub struct StressHttpTool {
    client: Client,
    call_count: Arc<AtomicUsize>,
    failure_count: Arc<AtomicUsize>,
}

impl StressHttpTool {
    pub fn new(call_count: Arc<AtomicUsize>, failure_count: Arc<AtomicUsize>) -> Self {
        Self {
            client: Client::new(),
            call_count,
            failure_count,
        }
    }
}

#[async_trait]
impl Tool for StressHttpTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Stress-local HTTP request tool backed by mock server"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "method": { "type": "string" },
                "url": { "type": "string" },
                "headers": { "type": "object" },
                "body": {}
            },
            "required": ["method", "url"]
        })
    }

    async fn execute(&self, input: Value) -> ai::tools::ToolResult<ToolOutput> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let params: HttpToolInput = serde_json::from_value(input)
            .map_err(|error| anyhow::anyhow!("invalid stress http_request input: {error}"))?;

        let method = params
            .method
            .parse::<Method>()
            .map_err(|error| anyhow::anyhow!("invalid method: {error}"))?;
        let mut request = self.client.request(method, &params.url);
        if let Some(body) = params.body {
            request = request.json(&body);
        }

        let response = request.send().await.map_err(anyhow::Error::from)?;
        let status = response.status();
        let text = response.text().await.map_err(anyhow::Error::from)?;

        if status.is_success() {
            return Ok(ToolOutput::success(json!({
                "status": status.as_u16(),
                "body": text,
            })));
        }

        self.failure_count.fetch_add(1, Ordering::SeqCst);
        let category = match status.as_u16() {
            429 => ToolErrorCategory::RateLimit,
            500..=599 => ToolErrorCategory::Network,
            _ => ToolErrorCategory::Execution,
        };
        let message = format!("HTTP {}: {}", status.as_u16(), text);
        Ok(if status.as_u16() == 429 || status.is_server_error() {
            ToolOutput::retryable_error(message, category)
        } else {
            ToolOutput::non_retryable_error(message, category)
        })
    }
}

pub struct StressBashTool {
    call_count: Arc<AtomicUsize>,
    failure_count: Arc<AtomicUsize>,
}

impl StressBashTool {
    pub fn new(call_count: Arc<AtomicUsize>, failure_count: Arc<AtomicUsize>) -> Self {
        Self {
            call_count,
            failure_count,
        }
    }
}

pub struct StressFileTool {
    call_count: Arc<AtomicUsize>,
    failure_count: Arc<AtomicUsize>,
}

impl StressFileTool {
    pub fn new(call_count: Arc<AtomicUsize>, failure_count: Arc<AtomicUsize>) -> Self {
        Self {
            call_count,
            failure_count,
        }
    }
}

pub struct StressPythonTool {
    call_count: Arc<AtomicUsize>,
    failure_count: Arc<AtomicUsize>,
}

impl StressPythonTool {
    pub fn new(call_count: Arc<AtomicUsize>, failure_count: Arc<AtomicUsize>) -> Self {
        Self {
            call_count,
            failure_count,
        }
    }
}

#[derive(Debug, Deserialize)]
struct BashInput {
    command: String,
    workdir: Option<String>,
}

#[async_trait]
impl Tool for StressBashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Stress-local bash tool"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "workdir": { "type": "string" }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, input: Value) -> ai::tools::ToolResult<ToolOutput> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let params: BashInput = serde_json::from_value(input)?;
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(params.command);
        if let Some(workdir) = params.workdir {
            cmd.current_dir(workdir);
        }
        let output = cmd.output().await.map_err(anyhow::Error::from)?;
        if output.status.success() {
            return Ok(ToolOutput::success(json!({
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "exit_code": output.status.code().unwrap_or(0),
            })));
        }
        self.failure_count.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::non_retryable_error(
            String::from_utf8_lossy(&output.stderr).to_string(),
            ToolErrorCategory::Execution,
        ))
    }
}

#[async_trait]
impl Tool for StressFileTool {
    fn name(&self) -> &str {
        "file"
    }

    fn description(&self) -> &str {
        "Stress-local file tool"
    }

    fn parameters_schema(&self) -> Value {
        json!({"type":"object"})
    }

    async fn execute(&self, input: Value) -> ai::tools::ToolResult<ToolOutput> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let action = input
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing action"))?;
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing path"))?;
        match action {
            "write" => {
                let content = input
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let append = input
                    .get("append")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(parent) = std::path::Path::new(path).parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(anyhow::Error::from)?;
                }
                let mut file = if append {
                    fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .await
                        .map_err(anyhow::Error::from)?
                } else {
                    fs::File::create(path).await.map_err(anyhow::Error::from)?
                };
                file.write_all(content.as_bytes())
                    .await
                    .map_err(anyhow::Error::from)?;
                Ok(ToolOutput::success(
                    json!({"path": path, "bytes": content.len()}),
                ))
            }
            "read" => {
                let content = fs::read_to_string(path)
                    .await
                    .map_err(anyhow::Error::from)?;
                Ok(ToolOutput::success(
                    json!({"path": path, "content": content}),
                ))
            }
            _ => {
                self.failure_count.fetch_add(1, Ordering::SeqCst);
                Ok(ToolOutput::non_retryable_error(
                    format!("unsupported file action: {action}"),
                    ToolErrorCategory::Config,
                ))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct PythonInput {
    code: String,
}

#[async_trait]
impl Tool for StressPythonTool {
    fn name(&self) -> &str {
        "run_skill"
    }

    fn description(&self) -> &str {
        "Stress-local python tool"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"code":{"type":"string"}},
            "required":["code"]
        })
    }

    async fn execute(&self, input: Value) -> ai::tools::ToolResult<ToolOutput> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let params: PythonInput = serde_json::from_value(input)?;
        let executable = std::env::var("RESTFLOW_STRESS_PYTHON_EXECUTABLE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "python3".to_string());
        let output = Command::new(executable)
            .arg("-c")
            .arg(params.code)
            .output()
            .await
            .map_err(anyhow::Error::from)?;
        if output.status.success() {
            return Ok(ToolOutput::success(json!({
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "exit_code": output.status.code().unwrap_or(0),
            })));
        }
        self.failure_count.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::non_retryable_error(
            String::from_utf8_lossy(&output.stderr).to_string(),
            ToolErrorCategory::Execution,
        ))
    }
}
