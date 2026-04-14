#![cfg(feature = "test-utils")]

mod stress_support;

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use restflow_ai::llm::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, LlmClientFactory, StreamChunk,
    StreamResult, TokenUsage,
};
use restflow_core::daemon::{IpcRequest, IpcResponse, StreamFrame, run_mcp_http_server};
use restflow_core::prompt_files;
use restflow_core::runtime::background_agent::install_test_llm_factory;
use restflow_core::{
    AppCore, ChatRole, ChatSession, ExecutionThread, ExecutionTraceStats, ModelId,
};
use restflow_models::ClientKind;
use restflow_traits::llm::LlmProvider;
use serde::de::DeserializeOwned;
use stress_support::{ProviderFamily, StreamMode, StressLevel, chat_smoke_profiles, rounds_for};
use tempfile::tempdir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

#[derive(Clone, Debug)]
struct IpcStressLlmClient {
    provider: &'static str,
    model: String,
    stream_mode: StreamMode,
}

impl IpcStressLlmClient {
    fn new(provider: &'static str, model: String, stream_mode: StreamMode) -> Self {
        Self {
            provider,
            model,
            stream_mode,
        }
    }

    fn usage_for(content: &str) -> TokenUsage {
        let completion_tokens = content.len() as u32;
        TokenUsage {
            prompt_tokens: 8,
            completion_tokens,
            total_tokens: completion_tokens + 8,
            cost_usd: Some(0.0),
        }
    }

    fn build_content(&self, request: &CompletionRequest) -> String {
        let is_ack = request
            .messages
            .iter()
            .any(|message| message.content.contains("Temporary Acknowledgement Phase"));
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, restflow_ai::Role::User))
            .map(|message| message.content.trim().to_string())
            .unwrap_or_else(|| "empty-input".to_string());

        if is_ack {
            format!("Starting {}", last_user)
        } else {
            format!("{}:{} completed {}", self.provider, self.model, last_user)
        }
    }
}

#[async_trait]
impl LlmClient for IpcStressLlmClient {
    fn provider(&self) -> &str {
        self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> restflow_ai::Result<CompletionResponse> {
        let content = self.build_content(&request);
        Ok(CompletionResponse {
            content: Some(content.clone()),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: Some(Self::usage_for(&content)),
        })
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let content = self.build_content(&request);
        let usage = Some(Self::usage_for(&content));
        let chunks = match self.stream_mode {
            StreamMode::NonStreaming => vec![
                Ok(StreamChunk::text(content)),
                Ok(StreamChunk::final_chunk(FinishReason::Stop, usage)),
            ],
            StreamMode::CoarseStreaming => vec![
                Ok(StreamChunk::text(content)),
                Ok(StreamChunk::final_chunk(FinishReason::Stop, usage)),
            ],
            StreamMode::Streaming => {
                let mut items = content
                    .split_whitespace()
                    .map(|part| Ok(StreamChunk::text(format!("{part} "))))
                    .collect::<Vec<_>>();
                items.push(Ok(StreamChunk::final_chunk(FinishReason::Stop, usage)));
                items
            }
        };
        Box::pin(futures::stream::iter(chunks))
    }

    fn supports_streaming(&self) -> bool {
        self.stream_mode != StreamMode::NonStreaming
    }
}

#[derive(Clone)]
struct IpcStressLlmFactory {
    models: HashMap<String, (ProviderFamily, StreamMode)>,
}

impl IpcStressLlmFactory {
    fn new() -> Self {
        let models = chat_smoke_profiles()
            .into_iter()
            .map(|profile| {
                (
                    profile.model_id.to_string(),
                    (profile.provider, profile.stream_mode),
                )
            })
            .collect();
        Self { models }
    }

    fn resolve_profile(&self, model: &str) -> Option<(ProviderFamily, StreamMode)> {
        let normalized = model.trim().to_lowercase();
        if let Some(profile) = self
            .models
            .iter()
            .find(|(name, _)| name.to_lowercase() == normalized)
            .map(|(_, profile)| *profile)
        {
            return Some(profile);
        }

        let model_id = ModelId::from_api_name(model)
            .or_else(|| ModelId::from_serialized_str(model))
            .or_else(|| ModelId::from_canonical_id(model))?;
        let provider = match model_id.provider() {
            restflow_core::Provider::OpenAI => ProviderFamily::OpenAI,
            restflow_core::Provider::Anthropic => ProviderFamily::Anthropic,
            restflow_core::Provider::MiniMax | restflow_core::Provider::MiniMaxCodingPlan => {
                ProviderFamily::MiniMaxShim
            }
            restflow_core::Provider::Google => ProviderFamily::Gemini,
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

impl LlmClientFactory for IpcStressLlmFactory {
    fn create_client(
        &self,
        model: &str,
        _api_key: Option<&str>,
    ) -> restflow_ai::Result<Arc<dyn LlmClient>> {
        let (provider, stream_mode) = self
            .resolve_profile(model)
            .ok_or_else(|| restflow_ai::AiError::Llm(format!("Unknown test model '{model}'")))?;
        Ok(Arc::new(IpcStressLlmClient::new(
            provider.as_str(),
            model.to_string(),
            stream_mode,
        )))
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

struct AgentsDirEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl AgentsDirEnvGuard {
    fn new() -> Self {
        Self {
            _lock: prompt_files::agents_dir_env_lock(),
        }
    }
}

impl Drop for AgentsDirEnvGuard {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(prompt_files::AGENTS_DIR_ENV) };
    }
}

struct TestCoreEnv {
    _db_dir: tempfile::TempDir,
    _agents_dir: tempfile::TempDir,
    _env_guard: AgentsDirEnvGuard,
}

async fn create_test_core() -> (Arc<AppCore>, TestCoreEnv) {
    let env_guard = AgentsDirEnvGuard::new();
    let db_dir = tempdir().expect("db tempdir");
    let agents_dir = tempdir().expect("agents tempdir");
    unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, agents_dir.path()) };
    let db_path = db_dir.path().join("ipc-session-stress.db");
    let core = Arc::new(
        AppCore::new(db_path.to_str().expect("db path"))
            .await
            .unwrap(),
    );
    (
        core,
        TestCoreEnv {
            _db_dir: db_dir,
            _agents_dir: agents_dir,
            _env_guard: env_guard,
        },
    )
}

struct HttpDaemonHarness {
    base_url: String,
    shutdown_tx: broadcast::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl HttpDaemonHarness {
    async fn start(core: Arc<AppCore>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local port");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let task = tokio::spawn(run_mcp_http_server(core, addr, shutdown_rx));
        let base_url = format!("http://{}", addr);
        let client = Client::new();

        for _ in 0..50 {
            if let Ok(response) = client.get(format!("{base_url}/api/health")).send().await
                && response.status().is_success()
            {
                return Self {
                    base_url,
                    shutdown_tx,
                    task,
                };
            }
            sleep(Duration::from_millis(50)).await;
        }

        panic!("daemon HTTP server did not become ready");
    }

    async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = timeout(Duration::from_secs(2), self.task).await;
    }
}

async fn request_typed<T: DeserializeOwned>(
    client: &Client,
    base_url: &str,
    request: &IpcRequest,
) -> T {
    let response = client
        .post(format!("{base_url}/api/request"))
        .json(request)
        .send()
        .await
        .expect("ipc request");
    let envelope: IpcResponse = response.json().await.expect("ipc response json");
    match envelope {
        IpcResponse::Success(value) => serde_json::from_value(value).expect("typed response"),
        IpcResponse::Error(error) => panic!("expected success response, got error: {error:?}"),
        IpcResponse::Pong => panic!("expected success response, got pong"),
    }
}

async fn collect_stream_frames(
    client: &Client,
    base_url: &str,
    request: &IpcRequest,
) -> Vec<StreamFrame> {
    let response = client
        .post(format!("{base_url}/api/stream"))
        .json(request)
        .send()
        .await
        .expect("stream request");
    assert!(response.status().is_success(), "stream request failed");

    let mut frames = Vec::new();
    let mut buffer = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("stream chunk");
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline) = buffer.find('\n') {
            let line = buffer[..newline].trim().to_string();
            buffer.drain(..=newline);
            if line.is_empty() {
                continue;
            }
            let frame: StreamFrame = serde_json::from_str(&line).expect("stream frame json");
            let terminal = matches!(frame, StreamFrame::Done { .. });
            if let StreamFrame::Error(error) = &frame {
                panic!("unexpected stream error: {error:?}");
            }
            frames.push(frame);
            if terminal {
                return frames;
            }
        }
    }

    frames
}

struct IpcSessionOutcome {
    stream_data_frames: usize,
    assistant_messages: usize,
    trace_events: u64,
}

async fn run_ipc_session_workload(
    client: &Client,
    base_url: &str,
    profile: &stress_support::ModelProfile,
    session_index: usize,
    turns_per_session: usize,
) -> IpcSessionOutcome {
    let mut session: ChatSession = request_typed(
        client,
        base_url,
        &IpcRequest::CreateSession {
            agent_id: None,
            model: Some(profile.model_id.to_string()),
            name: Some(format!("IPC Stress {session_index}")),
            skill_id: None,
        },
    )
    .await;

    let mut total_data_frames = 0usize;
    let mut total_trace_events = 0u64;

    for turn in 0..turns_per_session {
        let stream_id = format!("ipc-stress-{session_index}-{turn}");
        let user_input = format!(
            "session {session_index} turn {turn} for {}",
            profile.model_id
        );
        let frames = collect_stream_frames(
            client,
            base_url,
            &IpcRequest::ExecuteChatSessionStream {
                session_id: session.id.clone(),
                user_input: Some(user_input),
                stream_id: stream_id.clone(),
            },
        )
        .await;

        assert!(
            matches!(frames.first(), Some(StreamFrame::Start { .. })),
            "expected start frame first for {stream_id}"
        );
        assert!(
            matches!(frames.last(), Some(StreamFrame::Done { .. })),
            "expected done frame last for {stream_id}"
        );

        let data_frames = frames
            .iter()
            .filter(|frame| matches!(frame, StreamFrame::Data { .. }))
            .count();
        assert!(data_frames > 0, "expected data frames for {stream_id}");
        total_data_frames += data_frames;

        let trace_stats: ExecutionTraceStats = request_typed(
            client,
            base_url,
            &IpcRequest::GetExecutionTraceStats {
                run_id: Some(stream_id.clone()),
                task_id: None,
            },
        )
        .await;
        assert!(
            trace_stats.total_events > 0,
            "expected execution trace events for {stream_id}"
        );
        total_trace_events += trace_stats.total_events;

        let thread: ExecutionThread = request_typed(
            client,
            base_url,
            &IpcRequest::GetExecutionRunThread {
                run_id: stream_id.clone(),
            },
        )
        .await;
        assert_eq!(thread.focus.run_id.as_deref(), Some(stream_id.as_str()));
        assert_eq!(
            thread.focus.session_id.as_deref(),
            Some(session.id.as_str())
        );
        assert!(
            !thread.timeline.events.is_empty(),
            "expected run timeline events"
        );

        session = request_typed(client, base_url, &IpcRequest::GetSession { id: session.id }).await;
        assert_eq!(
            session.model,
            ModelId::from_api_name(profile.model_id)
                .unwrap()
                .as_serialized_str()
        );
        assert_eq!(session.provider, profile.provider.as_str());
    }

    let assistant_messages = session
        .messages
        .iter()
        .filter(|message| message.role == ChatRole::Assistant && !message.content.trim().is_empty())
        .count();
    assert!(
        assistant_messages >= turns_per_session,
        "expected persisted assistant messages for {}",
        session.id
    );

    IpcSessionOutcome {
        stream_data_frames: total_data_frames,
        assistant_messages,
        trace_events: total_trace_events,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_ipc_session_streams_finalize_consistently() {
    let level = StressLevel::current();
    let (core, _env) = create_test_core().await;
    let factory_guard = install_test_llm_factory(Arc::new(IpcStressLlmFactory::new()));
    let server = HttpDaemonHarness::start(core).await;
    let client = Client::new();

    let session_count = rounds_for(level, 3, 8, 16);
    let turns_per_session = rounds_for(level, 2, 4, 6);
    let profiles = chat_smoke_profiles();

    let outcomes = futures::future::join_all((0..session_count).map(|index| {
        let client = client.clone();
        let base_url = server.base_url.clone();
        let profile = profiles[index % profiles.len()].clone();
        async move {
            run_ipc_session_workload(&client, &base_url, &profile, index, turns_per_session).await
        }
    }))
    .await;

    let total_data_frames: usize = outcomes
        .iter()
        .map(|outcome| outcome.stream_data_frames)
        .sum();
    let total_assistant_messages: usize = outcomes
        .iter()
        .map(|outcome| outcome.assistant_messages)
        .sum();
    let total_trace_events: u64 = outcomes.iter().map(|outcome| outcome.trace_events).sum();

    assert!(
        total_data_frames >= session_count * turns_per_session,
        "expected streamed data across all IPC sessions"
    );
    assert!(
        total_assistant_messages >= session_count * turns_per_session,
        "expected persisted assistant messages across all IPC sessions"
    );
    assert!(
        total_trace_events >= (session_count * turns_per_session) as u64,
        "expected execution trace coverage across all IPC sessions"
    );

    drop(factory_guard);
    server.shutdown().await;
}
