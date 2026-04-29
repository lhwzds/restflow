#![allow(dead_code)]

mod mock_backends;

use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::stream;
use restflow_ai::agent::{SubagentConfig, SubagentTracker};
use restflow_ai::llm::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamChunk, StreamResult,
    TokenUsage, ToolCall,
};
use restflow_ai::tools::{Tool, ToolOutput, ToolRegistry, ToolResult};
use restflow_core::auth::AuthProfileManager;
use restflow_core::models::{AgentNode, TaskSchedule, TaskStatus};
use restflow_core::process::ProcessRegistry;
use restflow_core::prompt_files;
use restflow_core::runtime::agent::install_test_tool_overrides;
use restflow_core::runtime::subagent::AgentDefinitionRegistry;
use restflow_core::runtime::task_runtime::install_test_llm_factory;
use restflow_core::runtime::task_runtime::runner::{AgentExecutor, ExecutionResult};
use restflow_core::runtime::task_runtime::testkit::{MockNotificationSender, create_test_storage};
use restflow_core::runtime::{AgentRuntimeExecutor, TaskRunner, TaskRunnerConfig};
use restflow_core::steer::SteerRegistry;
use restflow_core::storage::Storage;
use serde_json::{Value, json};
use tempfile::tempdir;
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;

#[allow(unused_imports)]
pub use mock_backends::{
    MockLlmHttpServer, MockToolHttpServer, StressBashTool, StressFileTool, StressHttpTool,
    StressPythonTool, StubHttpLlmFactory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionSurface {
    InteractiveChat,
    BackgroundTask,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StressLevel {
    Smoke,
    Stress,
    Soak,
}

impl StressLevel {
    pub fn current() -> Self {
        match env::var("RESTFLOW_STRESS_LEVEL").ok().as_deref() {
            Some("soak") => Self::Soak,
            Some("stress") => Self::Stress,
            _ => Self::Smoke,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFamily {
    OpenAI,
    Anthropic,
    MiniMaxShim,
    Gemini,
    CodexCli,
}

impl ProviderFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::MiniMaxShim => "minimax-coding-plan",
            Self::Gemini => "google",
            Self::CodexCli => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    Streaming,
    CoarseStreaming,
    NonStreaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFamily {
    Code,
    State,
    Coordination,
    Io,
    TaskMgmt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureMode {
    Never,
    RetryableEvery(u32),
    FatalEvery(u32),
}

#[derive(Debug, Clone)]
pub struct ModelProfile {
    pub provider: ProviderFamily,
    pub model_id: &'static str,
    pub stream_mode: StreamMode,
    pub tool_density: usize,
}

#[derive(Debug, Clone)]
pub struct ToolProfile {
    pub family: ToolFamily,
    pub name: &'static str,
    pub latency_ms: u64,
    pub failure_mode: FailureMode,
    pub output_size: usize,
}

#[derive(Debug, Clone)]
pub struct WorkloadProfile {
    pub name: &'static str,
    pub surface: ExecutionSurface,
    pub rounds: usize,
    pub concurrency: usize,
    pub models: Vec<ModelProfile>,
    pub tools: Vec<ToolProfile>,
}

#[derive(Debug, Clone, Default)]
pub struct StressSummary {
    pub total_runs: usize,
    pub non_empty_outputs: usize,
    pub tool_calls: usize,
    pub tool_failures: usize,
    pub completed: usize,
    pub failed: usize,
    pub notifications_sent: usize,
    pub provider_switches: usize,
    pub failover_count: usize,
    pub orphan_running: usize,
    pub recovery_elapsed_ms: Option<u64>,
    pub sample_errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct ScriptedLlmClient {
    provider: &'static str,
    model: &'static str,
    stream_mode: StreamMode,
    responses: Arc<Mutex<VecDeque<CompletionResponse>>>,
}

impl ScriptedLlmClient {
    fn new(
        provider: &'static str,
        model: &'static str,
        stream_mode: StreamMode,
        responses: Vec<CompletionResponse>,
    ) -> Self {
        Self {
            provider,
            model,
            stream_mode,
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }

    fn usage_for(content: &str) -> TokenUsage {
        let completion_tokens = content.len() as u32;
        TokenUsage {
            prompt_tokens: 1,
            completion_tokens,
            total_tokens: completion_tokens + 1,
            cost_usd: Some(0.0),
        }
    }
}

#[async_trait]
impl LlmClient for ScriptedLlmClient {
    fn provider(&self) -> &str {
        self.provider
    }

    fn model(&self) -> &str {
        self.model
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> restflow_ai::Result<CompletionResponse> {
        if let Some(response) = self.responses.lock().await.pop_front() {
            return Ok(response);
        }

        let content = request
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, restflow_ai::Role::User))
            .map(|message| format!("mock-echo: {}", message.content))
            .unwrap_or_else(|| "mock-ok".to_string());
        Ok(CompletionResponse {
            content: Some(content.clone()),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: Some(Self::usage_for(&content)),
        })
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        let client = self.clone();
        let result = futures::executor::block_on(client.complete(request));
        let items = match result {
            Ok(response) => {
                let content = response.content.unwrap_or_default();
                let mut chunks = Vec::new();
                match client.stream_mode {
                    StreamMode::NonStreaming => {}
                    StreamMode::CoarseStreaming => {
                        if !content.is_empty() {
                            chunks.push(Ok(StreamChunk::text(content)));
                        }
                    }
                    StreamMode::Streaming => {
                        for chunk in content.split_whitespace() {
                            chunks.push(Ok(StreamChunk::text(format!("{chunk} "))));
                        }
                    }
                }
                chunks.push(Ok(StreamChunk::final_chunk(
                    response.finish_reason,
                    response.usage,
                )));
                chunks
            }
            Err(err) => vec![Err(err)],
        };
        Box::pin(stream::iter(items))
    }

    fn supports_streaming(&self) -> bool {
        self.stream_mode != StreamMode::NonStreaming
    }
}

#[derive(Debug)]
struct ProfiledTool {
    profile: ToolProfile,
    call_count: Arc<AtomicU32>,
    failure_count: Arc<AtomicU32>,
}

impl ProfiledTool {
    fn new(
        profile: ToolProfile,
        call_count: Arc<AtomicU32>,
        failure_count: Arc<AtomicU32>,
    ) -> Self {
        Self {
            profile,
            call_count,
            failure_count,
        }
    }

    fn should_fail(&self, call_index: u32) -> bool {
        match self.profile.failure_mode {
            FailureMode::Never => false,
            FailureMode::RetryableEvery(interval) | FailureMode::FatalEvery(interval) => {
                interval > 0 && call_index.is_multiple_of(interval)
            }
        }
    }
}

#[async_trait]
impl Tool for ProfiledTool {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn description(&self) -> &str {
        "Profiled stress tool"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            }
        })
    }

    async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
        let call_index = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.profile.latency_ms > 0 {
            sleep(Duration::from_millis(self.profile.latency_ms)).await;
        }
        if self.should_fail(call_index) {
            self.failure_count.fetch_add(1, Ordering::SeqCst);
            let message = match self.profile.failure_mode {
                FailureMode::RetryableEvery(_) => "retryable stress tool failure",
                FailureMode::FatalEvery(_) => "fatal stress tool failure",
                FailureMode::Never => "unexpected stress failure",
            };
            return Ok(ToolOutput::error(message));
        }

        let payload = input
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("ok")
            .repeat(self.profile.output_size.max(1));
        Ok(ToolOutput::success(json!({
            "tool": self.profile.name,
            "family": format!("{:?}", self.profile.family),
            "payload": payload,
        })))
    }
}

#[derive(Debug)]
pub struct ProviderAwareMockExecutor {
    pub model_profile: ModelProfile,
    pub delay_ms: u64,
    pub failure_mode: FailureMode,
    call_count: AtomicU32,
}

impl ProviderAwareMockExecutor {
    pub fn new(model_profile: ModelProfile, delay_ms: u64, failure_mode: FailureMode) -> Self {
        Self {
            model_profile,
            delay_ms,
            failure_mode,
            call_count: AtomicU32::new(0),
        }
    }

    pub fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AgentExecutor for ProviderAwareMockExecutor {
    async fn execute(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        _memory_config: &restflow_core::models::MemoryConfig,
        _steer_rx: Option<mpsc::Receiver<restflow_core::models::SteerMessage>>,
    ) -> Result<ExecutionResult> {
        let call_index = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }
        let should_fail = match self.failure_mode {
            FailureMode::Never => false,
            FailureMode::RetryableEvery(interval) | FailureMode::FatalEvery(interval) => {
                interval > 0 && call_index.is_multiple_of(interval)
            }
        };
        if should_fail {
            return Err(anyhow!(
                "{} failure for {} on attempt {} ({:?})",
                self.model_profile.provider.as_str(),
                background_task_id.unwrap_or("unknown-task"),
                call_index,
                self.failure_mode
            ));
        }

        Ok(ExecutionResult::success(
            format!(
                "provider={} model={} agent={} input={}",
                self.model_profile.provider.as_str(),
                self.model_profile.model_id,
                agent_id,
                input.unwrap_or_default()
            ),
            Vec::new(),
        ))
    }
}

pub fn default_tool_profiles() -> Vec<ToolProfile> {
    vec![
        ToolProfile {
            family: ToolFamily::Code,
            name: "echo_code",
            latency_ms: 5,
            failure_mode: FailureMode::Never,
            output_size: 1,
        },
        ToolProfile {
            family: ToolFamily::State,
            name: "echo_state",
            latency_ms: 2,
            failure_mode: FailureMode::Never,
            output_size: 1,
        },
        ToolProfile {
            family: ToolFamily::Coordination,
            name: "echo_coordination",
            latency_ms: 3,
            failure_mode: FailureMode::Never,
            output_size: 1,
        },
        ToolProfile {
            family: ToolFamily::Io,
            name: "echo_io",
            latency_ms: 12,
            failure_mode: FailureMode::RetryableEvery(7),
            output_size: 8,
        },
        ToolProfile {
            family: ToolFamily::TaskMgmt,
            name: "echo_task",
            latency_ms: 6,
            failure_mode: FailureMode::FatalEvery(11),
            output_size: 2,
        },
    ]
}

pub fn coordination_tool_profiles() -> Vec<ToolProfile> {
    vec![
        ToolProfile {
            family: ToolFamily::Coordination,
            name: "spawn_subagent",
            latency_ms: 4,
            failure_mode: FailureMode::Never,
            output_size: 1,
        },
        ToolProfile {
            family: ToolFamily::Coordination,
            name: "wait_subagents",
            latency_ms: 4,
            failure_mode: FailureMode::Never,
            output_size: 1,
        },
        ToolProfile {
            family: ToolFamily::Coordination,
            name: "switch_model",
            latency_ms: 2,
            failure_mode: FailureMode::Never,
            output_size: 1,
        },
    ]
}

pub fn chat_smoke_profiles() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            provider: ProviderFamily::OpenAI,
            model_id: "gpt-5",
            stream_mode: StreamMode::Streaming,
            tool_density: 2,
        },
        ModelProfile {
            provider: ProviderFamily::Anthropic,
            model_id: "claude-sonnet-4-5",
            stream_mode: StreamMode::Streaming,
            tool_density: 1,
        },
        ModelProfile {
            provider: ProviderFamily::MiniMaxShim,
            model_id: "minimax-coding-plan-m2-5",
            stream_mode: StreamMode::CoarseStreaming,
            tool_density: 1,
        },
        ModelProfile {
            provider: ProviderFamily::Gemini,
            model_id: "gemini-2.5-pro",
            stream_mode: StreamMode::Streaming,
            tool_density: 2,
        },
    ]
}

pub fn background_smoke_profiles() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            provider: ProviderFamily::CodexCli,
            model_id: "gpt-5.3-codex",
            stream_mode: StreamMode::NonStreaming,
            tool_density: 0,
        },
        ModelProfile {
            provider: ProviderFamily::OpenAI,
            model_id: "gpt-5",
            stream_mode: StreamMode::Streaming,
            tool_density: 0,
        },
    ]
}

pub fn rounds_for(level: StressLevel, smoke: usize, stress: usize, soak: usize) -> usize {
    match level {
        StressLevel::Smoke => smoke,
        StressLevel::Stress => stress,
        StressLevel::Soak => soak,
    }
}

pub fn task_count_for(level: StressLevel, smoke: usize, stress: usize, soak: usize) -> usize {
    match level {
        StressLevel::Smoke => smoke,
        StressLevel::Stress => stress,
        StressLevel::Soak => soak,
    }
}

pub fn timeout_for(
    level: StressLevel,
    smoke: Duration,
    stress: Duration,
    soak: Duration,
) -> Duration {
    match level {
        StressLevel::Smoke => smoke,
        StressLevel::Stress => stress,
        StressLevel::Soak => soak,
    }
}

#[derive(Clone)]
pub struct ToolCounters {
    pub name: &'static str,
    pub calls: Arc<AtomicU32>,
    pub failures: Arc<AtomicU32>,
}

pub fn install_real_io_tool_overrides() -> (
    restflow_core::runtime::agent::TestToolOverrideGuard,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    let tool_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tool_failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let guard = install_test_tool_overrides(HashMap::from([
        (
            "http_request".to_string(),
            Arc::new(StressHttpTool::new(
                tool_calls.clone(),
                tool_failures.clone(),
            )) as Arc<dyn restflow_ai::tools::Tool>,
        ),
        (
            "bash".to_string(),
            Arc::new(StressBashTool::new(
                tool_calls.clone(),
                tool_failures.clone(),
            )) as Arc<dyn restflow_ai::tools::Tool>,
        ),
        (
            "file".to_string(),
            Arc::new(StressFileTool::new(
                tool_calls.clone(),
                tool_failures.clone(),
            )) as Arc<dyn restflow_ai::tools::Tool>,
        ),
        (
            "run_python".to_string(),
            Arc::new(StressPythonTool::new(
                tool_calls.clone(),
                tool_failures.clone(),
            )) as Arc<dyn restflow_ai::tools::Tool>,
        ),
    ]));
    (guard, tool_calls, tool_failures)
}

pub struct AgentsDirEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl AgentsDirEnvGuard {
    pub fn new() -> Self {
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

pub fn build_tool_registry(
    tool_profiles: &[ToolProfile],
) -> (Arc<ToolRegistry>, Vec<ToolCounters>) {
    let mut registry = ToolRegistry::new();
    let mut counters = Vec::new();
    for profile in tool_profiles.iter().cloned() {
        let calls = Arc::new(AtomicU32::new(0));
        let failures = Arc::new(AtomicU32::new(0));
        registry.register(ProfiledTool::new(
            profile.clone(),
            calls.clone(),
            failures.clone(),
        ));
        counters.push(ToolCounters {
            name: profile.name,
            calls,
            failures,
        });
    }
    (Arc::new(registry), counters)
}

pub async fn run_chat_workload(profile: &ModelProfile, rounds: usize) -> StressSummary {
    run_chat_workload_with_tools(profile, rounds, default_tool_profiles()).await
}

pub async fn run_chat_workload_with_tools(
    profile: &ModelProfile,
    rounds: usize,
    tool_profiles: Vec<ToolProfile>,
) -> StressSummary {
    let mut summary = StressSummary::default();
    let tool_profiles = tool_profiles
        .into_iter()
        .take(profile.tool_density.max(1))
        .collect::<Vec<_>>();
    let (tools, counters) = build_tool_registry(&tool_profiles);

    for round in 0..rounds {
        let responses = if profile.tool_density > 0 {
            vec![
                CompletionResponse {
                    content: Some("calling tool".to_string()),
                    tool_calls: vec![ToolCall {
                        id: format!("call-{round}"),
                        name: tool_profiles[round % tool_profiles.len()].name.to_string(),
                        arguments: json!({ "message": format!("round-{round}") }),
                    }],
                    finish_reason: FinishReason::ToolCalls,
                    usage: None,
                },
                CompletionResponse {
                    content: Some(format!("done round {round}")),
                    tool_calls: Vec::new(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
            ]
        } else {
            vec![CompletionResponse {
                content: Some(format!("done round {round}")),
                tool_calls: Vec::new(),
                finish_reason: FinishReason::Stop,
                usage: None,
            }]
        };
        let llm = Arc::new(ScriptedLlmClient::new(
            profile.provider.as_str(),
            profile.model_id,
            profile.stream_mode,
            responses,
        ));
        let executor = restflow_ai::AgentExecutor::new(llm, tools.clone());
        let result = executor
            .run(restflow_ai::AgentConfig::new(format!(
                "stress chat round {round}"
            )))
            .await
            .expect("chat workload should execute");
        summary.total_runs += 1;
        if result.success {
            summary.completed += 1;
        } else {
            summary.failed += 1;
        }
        if result
            .answer
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
        {
            summary.non_empty_outputs += 1;
        }
    }

    summary.tool_calls = counters
        .iter()
        .map(|counter| counter.calls.load(Ordering::SeqCst) as usize)
        .sum();
    summary.tool_failures = counters
        .iter()
        .map(|counter| counter.failures.load(Ordering::SeqCst) as usize)
        .sum();
    summary.provider_switches = counters
        .iter()
        .find(|counter| counter.name == "switch_model")
        .map(|counter| counter.calls.load(Ordering::SeqCst) as usize)
        .unwrap_or(0);
    summary
}

pub async fn run_chat_workload_with_real_http_io(
    profile: &ModelProfile,
    rounds: usize,
    tool_server: &MockToolHttpServer,
) -> StressSummary {
    let mut summary = StressSummary::default();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(StressHttpTool::new(calls.clone(), failures.clone()));
    let tools = Arc::new(registry);

    for round in 0..rounds {
        let url = tool_server.url_for_round(round);
        let llm = Arc::new(ScriptedLlmClient::new(
            profile.provider.as_str(),
            profile.model_id,
            profile.stream_mode,
            vec![
                CompletionResponse {
                    content: Some("calling http_request".to_string()),
                    tool_calls: vec![ToolCall {
                        id: format!("http-call-{round}"),
                        name: "http_request".to_string(),
                        arguments: json!({
                            "method": if round % 2 == 0 { "GET" } else { "POST" },
                            "url": url,
                            "body": { "round": round, "model": profile.model_id }
                        }),
                    }],
                    finish_reason: FinishReason::ToolCalls,
                    usage: None,
                },
                CompletionResponse {
                    content: Some(format!("done round {round}")),
                    tool_calls: Vec::new(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
            ],
        ));
        let executor = restflow_ai::AgentExecutor::new(llm, tools.clone());
        let result = executor
            .run(restflow_ai::AgentConfig::new(format!(
                "real http io stress round {round}"
            )))
            .await
            .expect("real http io workload should execute");
        summary.total_runs += 1;
        if result.success {
            summary.completed += 1;
        } else {
            summary.failed += 1;
        }
        if result
            .answer
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
        {
            summary.non_empty_outputs += 1;
        }
    }

    summary.tool_calls = calls.load(Ordering::SeqCst);
    summary.tool_failures = failures.load(Ordering::SeqCst);
    summary
}

pub async fn run_background_workload(
    profile: &ModelProfile,
    task_count: usize,
    failure_mode: FailureMode,
) -> StressSummary {
    let level = StressLevel::current();
    let (storage, _temp_dir) = create_test_storage();
    let now = chrono::Utc::now().timestamp_millis();
    for index in 0..task_count {
        let mut task = storage
            .create_task(
                format!("{}-task-{index}", profile.provider.as_str()),
                "agent-mock".to_string(),
                TaskSchedule::Once {
                    run_at: now + 1_000,
                },
            )
            .expect("create background stress task");
        task.input = Some(format!("background-input-{index}"));
        task.next_run_at = Some(now - 1_000);
        storage
            .update_task(&task)
            .expect("update background stress task");
    }

    let executor = Arc::new(ProviderAwareMockExecutor::new(
        profile.clone(),
        5,
        failure_mode,
    ));
    let notifier = Arc::new(MockNotificationSender::new());
    let max_concurrent = match level {
        StressLevel::Smoke => task_count.min(8),
        StressLevel::Stress => task_count.min(12),
        StressLevel::Soak => task_count.min(16),
    };
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier.clone(),
        TaskRunnerConfig {
            poll_interval_ms: 20,
            max_concurrent_tasks: max_concurrent,
            worker_count: max_concurrent,
            task_timeout_secs: Some(30),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle = runner.clone().start();
    let deadline = tokio::time::Instant::now()
        + timeout_for(
            level,
            Duration::from_secs(15),
            Duration::from_secs(45),
            Duration::from_secs(180),
        );
    loop {
        let tasks = storage.list_tasks().expect("list background stress tasks");
        let terminal = tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Completed | TaskStatus::Failed))
            .count();
        if terminal == task_count {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("background workload timed out before all tasks reached terminal state");
        }
        sleep(Duration::from_millis(50)).await;
    }
    handle.stop().await.expect("stop background stress runner");

    let tasks = storage
        .list_tasks()
        .expect("load background stress results");
    let orphan_running = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Running)
        .count();
    StressSummary {
        total_runs: task_count,
        non_empty_outputs: tasks
            .iter()
            .filter(|task| task.last_error.is_none())
            .count(),
        tool_calls: 0,
        tool_failures: 0,
        completed: tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Completed)
            .count(),
        failed: tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Failed)
            .count(),
        notifications_sent: notifier.notification_count().await,
        orphan_running,
        failover_count: 0,
        provider_switches: 0,
        recovery_elapsed_ms: None,
        sample_errors: tasks
            .iter()
            .filter_map(|task| task.last_error.clone())
            .take(3)
            .collect(),
    }
}

fn create_full_test_storage() -> (Arc<Storage>, tempfile::TempDir) {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("stress-real-runtime.db");
    let storage = Storage::new(db_path.to_str().expect("db path")).expect("create storage");
    (Arc::new(storage), temp_dir)
}

fn create_real_background_executor(storage: Arc<Storage>) -> AgentRuntimeExecutor {
    let auth_manager = Arc::new(AuthProfileManager::new(Arc::new(storage.secrets.clone())));
    let (completion_tx, completion_rx) = mpsc::channel(32);
    let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
    let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
    AgentRuntimeExecutor::new(
        storage,
        Arc::new(ProcessRegistry::new()),
        auth_manager,
        subagent_tracker,
        subagent_definitions,
        SubagentConfig::default(),
    )
}

pub async fn run_background_workload_with_real_runtime(
    profile: &ModelProfile,
    level: StressLevel,
    task_count: usize,
    llm_server: &MockLlmHttpServer,
    tool_server: &MockToolHttpServer,
) -> StressSummary {
    let env_guard = AgentsDirEnvGuard::new();
    let agents_dir = tempdir().expect("agents tempdir");
    unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, agents_dir.path()) };

    let (storage, _temp_dir) = create_full_test_storage();
    let model = restflow_core::ModelId::from_api_name(profile.model_id)
        .or_else(|| restflow_core::ModelId::from_serialized_str(profile.model_id))
        .expect("known model");
    let agent = storage
        .agents
        .create_agent(
            format!("stress-agent-{}", profile.model_id),
            AgentNode {
                model: Some(model),
                tools: Some(vec![
                    "http_request".to_string(),
                    "bash".to_string(),
                    "file".to_string(),
                    "run_python".to_string(),
                ]),
                ..AgentNode::new()
            },
        )
        .expect("create stress task agent");

    let (_tool_guard, tool_calls, tool_failures) = install_real_io_tool_overrides();
    let _llm_guard = install_test_llm_factory(Arc::new(StubHttpLlmFactory::new(
        llm_server.base_url().to_string(),
        chat_smoke_profiles().into_iter().map(|profile| {
            (
                profile.model_id.to_string(),
                (profile.provider, profile.stream_mode),
            )
        }),
    )));

    let now = chrono::Utc::now().timestamp_millis();
    for index in 0..task_count {
        let mut task = storage
            .tasks
            .create_task(
                format!("real-{}-task-{index}", profile.provider.as_str()),
                agent.id.clone(),
                TaskSchedule::Once {
                    run_at: now + 1_000,
                },
            )
            .expect("create real runtime background task");
        task.input = Some(format!(
            "background real task {index} for {} tool_url={} tool_file_path={}/background-{}.txt tool_workdir={} tool_steps={} payload_words={}",
            profile.model_id,
            tool_server.stable_url_for_round(index),
            std::env::temp_dir().display(),
            index,
            std::env::temp_dir().display(),
            rounds_for(level, 3, 4, 6),
            rounds_for(level, 128, 256, 512)
        ));
        task.next_run_at = Some(now - 1_000);
        storage
            .tasks
            .update_task(&task)
            .expect("update real background task");
    }

    let executor = Arc::new(create_real_background_executor(storage.clone()));
    let notifier = Arc::new(MockNotificationSender::new());
    let max_concurrent = match level {
        StressLevel::Smoke => task_count.min(4),
        StressLevel::Stress => task_count.min(32),
        StressLevel::Soak => task_count.min(64),
    };
    let runner = Arc::new(TaskRunner::new(
        Arc::new(storage.tasks.clone()),
        executor,
        notifier.clone(),
        TaskRunnerConfig {
            poll_interval_ms: 20,
            max_concurrent_tasks: max_concurrent,
            worker_count: max_concurrent,
            task_timeout_secs: Some(60),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle = runner.clone().start();
    let deadline = tokio::time::Instant::now()
        + timeout_for(
            level,
            Duration::from_secs(20),
            Duration::from_secs(90),
            Duration::from_secs(240),
        );
    loop {
        let tasks = storage.tasks.list_tasks().expect("list real runtime tasks");
        let terminal = tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Completed | TaskStatus::Failed))
            .count();
        if terminal == task_count {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("real runtime background workload timed out before terminal state");
        }
        sleep(Duration::from_millis(50)).await;
    }
    handle.stop().await.expect("stop real runtime task runner");

    let tasks = storage
        .tasks
        .list_tasks()
        .expect("load real runtime results");
    let orphan_running = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Running)
        .count();
    drop(env_guard);

    StressSummary {
        total_runs: task_count,
        non_empty_outputs: tasks
            .iter()
            .filter(|task| task.last_error.is_none())
            .count(),
        tool_calls: tool_calls.load(Ordering::SeqCst),
        tool_failures: tool_failures.load(Ordering::SeqCst),
        completed: tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Completed)
            .count(),
        failed: tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Failed)
            .count(),
        notifications_sent: notifier.notification_count().await,
        orphan_running,
        failover_count: 0,
        provider_switches: 0,
        recovery_elapsed_ms: None,
        sample_errors: tasks
            .iter()
            .filter_map(|task| task.last_error.clone())
            .take(3)
            .collect(),
    }
}

pub fn assert_terminal_coverage(summary: &StressSummary) {
    assert_eq!(
        summary.completed + summary.failed,
        summary.total_runs,
        "all runs should terminate"
    );
}

pub fn assert_non_empty_outputs(summary: &StressSummary) {
    assert_eq!(
        summary.non_empty_outputs, summary.completed,
        "all completed runs should have non-empty outputs"
    );
}

pub fn assert_notifications_within_attempt_budget(summary: &StressSummary) {
    assert!(
        summary.notifications_sent <= summary.total_runs,
        "notifications should not exceed total execution attempts"
    );
}

pub fn assert_no_orphan_running(summary: &StressSummary) {
    assert_eq!(summary.orphan_running, 0, "stress run leaked running tasks");
}

pub fn assert_tool_call_result_pairing(summary: &StressSummary) {
    assert!(
        summary.tool_calls >= summary.tool_failures,
        "tool failures cannot exceed tool invocations"
    );
}

pub fn assert_recovery_within_budget(summary: &StressSummary, max_recovery_ms: u64) {
    let elapsed = summary
        .recovery_elapsed_ms
        .expect("recovery elapsed should be recorded");
    assert!(
        elapsed <= max_recovery_ms,
        "recovery exceeded budget: {elapsed}ms > {max_recovery_ms}ms"
    );
}
