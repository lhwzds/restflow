#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use restflow_ai::llm::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamChunk, StreamResult,
    TokenUsage, ToolCall,
};
use restflow_ai::tools::{Tool, ToolOutput, ToolRegistry, ToolResult};
use restflow_core::models::{TaskSchedule, TaskStatus};
use restflow_core::runtime::background_agent::runner::{AgentExecutor, ExecutionResult};
use restflow_core::runtime::background_agent::testkit::{
    MockNotificationSender, create_test_storage,
};
use restflow_core::runtime::{TaskRunner, TaskRunnerConfig};
use restflow_core::steer::SteerRegistry;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;
use futures::stream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionSurface {
    InteractiveChat,
    BackgroundTask,
    Mixed,
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

    async fn complete(&self, request: CompletionRequest) -> restflow_ai::Result<CompletionResponse> {
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
}

impl ProfiledTool {
    fn new(profile: ToolProfile, call_count: Arc<AtomicU32>) -> Self {
        Self { profile, call_count }
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

pub fn build_tool_registry(tool_profiles: &[ToolProfile]) -> (Arc<ToolRegistry>, Vec<Arc<AtomicU32>>) {
    let mut registry = ToolRegistry::new();
    let mut counters = Vec::new();
    for profile in tool_profiles.iter().cloned() {
        let counter = Arc::new(AtomicU32::new(0));
        registry.register(ProfiledTool::new(profile, counter.clone()));
        counters.push(counter);
    }
    (Arc::new(registry), counters)
}

pub async fn run_chat_workload(profile: &ModelProfile, rounds: usize) -> StressSummary {
    let mut summary = StressSummary::default();
    let tool_profiles = default_tool_profiles()
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
            .run(restflow_ai::AgentConfig::new(format!("stress chat round {round}")))
            .await
            .expect("chat workload should execute");
        summary.total_runs += 1;
        if result.success {
            summary.completed += 1;
        } else {
            summary.failed += 1;
        }
        if result.answer.as_deref().is_some_and(|text| !text.trim().is_empty()) {
            summary.non_empty_outputs += 1;
        }
    }

    summary.tool_calls = counters
        .iter()
        .map(|counter| counter.load(Ordering::SeqCst) as usize)
        .sum();
    summary
}

pub async fn run_background_workload(
    profile: &ModelProfile,
    task_count: usize,
    failure_mode: FailureMode,
) -> StressSummary {
    let (storage, _temp_dir) = create_test_storage();
    let now = chrono::Utc::now().timestamp_millis();
    for index in 0..task_count {
        let mut task = storage
            .create_task(
                format!("{}-task-{index}", profile.provider.as_str()),
                "agent-mock".to_string(),
                TaskSchedule::Once { run_at: now + 1_000 },
            )
            .expect("create background stress task");
        task.input = Some(format!("background-input-{index}"));
        task.next_run_at = Some(now - 1_000);
        storage
            .update_task(&task)
            .expect("update background stress task");
    }

    let executor = Arc::new(ProviderAwareMockExecutor::new(profile.clone(), 5, failure_mode));
    let notifier = Arc::new(MockNotificationSender::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier.clone(),
        TaskRunnerConfig {
            poll_interval_ms: 20,
            max_concurrent_tasks: task_count.min(8),
            worker_count: task_count.min(8),
            task_timeout_secs: Some(30),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle = runner.clone().start();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
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

    let tasks = storage.list_tasks().expect("load background stress results");
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
        summary.non_empty_outputs,
        summary.completed,
        "all completed runs should have non-empty outputs"
    );
}

pub fn assert_notifications_within_attempt_budget(summary: &StressSummary) {
    assert!(
        summary.notifications_sent <= summary.total_runs,
        "notifications should not exceed total execution attempts"
    );
}
