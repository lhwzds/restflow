use super::steer::parse_approval_resolution;
use super::tool_exec::{ToolExecutionOptions, ToolInvocationContext};
use super::*;
use crate::agent::ExecutionStep;
use crate::agent::PromptFlags;
use crate::agent::context::{ContextDiscoveryConfig, WorkspaceContextCache};
use crate::llm::{
    CompletionRequest, CompletionResponse, FinishReason, Role, StreamChunk, StreamResult,
    TokenUsage, ToolCall,
};
use crate::tools::ToolResult;
use crate::tools::{Tool, ToolErrorCategory, ToolOutput};
use async_trait::async_trait;
use futures::{StreamExt, stream};
use restflow_telemetry::{ExecutionEvent, ExecutionEventEnvelope, TelemetryContext, TelemetrySink};
use restflow_traits::{ClientKind, LlmProvider};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use tokio::time::sleep;

/// Mock LLM client for testing
struct MockLlmClient {
    responses: Mutex<Vec<CompletionResponse>>,
    call_count: AtomicUsize,
    supports_streaming: bool,
    /// Captured requests for verification
    captured_requests: Mutex<Vec<Vec<Message>>>,
}

async fn cwd_lock() -> AsyncMutexGuard<'static, ()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(())).lock().await
}

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn set(path: &Path) -> Self {
        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(path).expect("set current dir");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
    }
}

impl MockLlmClient {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self::with_streaming(responses, true)
    }

    fn with_streaming(responses: Vec<CompletionResponse>, supports_streaming: bool) -> Self {
        Self {
            responses: Mutex::new(responses),
            call_count: AtomicUsize::new(0),
            supports_streaming,
            captured_requests: Mutex::new(Vec::new()),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn captured_requests(&self) -> Vec<Vec<Message>> {
        self.captured_requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "mock-model"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        // Capture the messages sent to the LLM
        self.captured_requests
            .lock()
            .unwrap()
            .push(request.messages.clone());

        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(CompletionResponse {
                content: Some("Done".to_string()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    cost_usd: None,
                }),
            })
        } else {
            Ok(responses.remove(0))
        }
    }

    fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
        // For mock: convert the sync response to a single-chunk stream
        let response = futures::executor::block_on(self.complete(request));
        match response {
            Ok(resp) => {
                let chunk = StreamChunk {
                    text: resp.content.unwrap_or_default(),
                    thinking: None,
                    tool_call_delta: None,
                    finish_reason: Some(resp.finish_reason),
                    usage: resp.usage,
                };
                Box::pin(stream::once(async move { Ok(chunk) }))
            }
            Err(e) => Box::pin(stream::once(async move { Err(e) })),
        }
    }

    fn supports_streaming(&self) -> bool {
        self.supports_streaming
    }
}

struct DelayedLlmClient {
    delay: std::time::Duration,
}

impl DelayedLlmClient {
    fn new(delay: std::time::Duration) -> Self {
        Self { delay }
    }
}

#[async_trait]
impl LlmClient for DelayedLlmClient {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "delayed-model"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        sleep(self.delay).await;
        Ok(CompletionResponse {
            content: Some("Delayed done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        })
    }

    fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
        panic!("streaming path is not used in delay timeout tests");
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

#[test]
fn sanitize_tool_call_history_drops_orphan_tool_results() {
    let messages = vec![
        Message::system("s"),
        Message::assistant_with_tool_calls(
            None,
            vec![ToolCall {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"cmd":"echo 1"}),
            }],
        ),
        Message::tool_result("call_1", "{\"ok\":true}"),
        Message::tool_result("orphan_call", "{\"ok\":false}"),
    ];

    let sanitized = sanitize_tool_call_history(messages);
    let tool_results: Vec<_> = sanitized
        .iter()
        .filter(|m| matches!(m.role, Role::Tool))
        .collect();
    assert_eq!(tool_results.len(), 1);
    assert_eq!(tool_results[0].tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn sanitize_tool_call_history_filters_assistant_orphan_tool_calls() {
    let messages = vec![
        Message::assistant_with_tool_calls(
            Some("planning".to_string()),
            vec![
                ToolCall {
                    id: "call_1".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"cmd":"echo 1"}),
                },
                ToolCall {
                    id: "call_2".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"cmd":"echo 2"}),
                },
            ],
        ),
        Message::tool_result("call_1", "{\"ok\":true}"),
    ];

    let sanitized = sanitize_tool_call_history(messages);
    let assistant = sanitized
        .iter()
        .find(|m| m.role == Role::Assistant)
        .expect("assistant message should exist");
    let tool_calls = assistant
        .tool_calls
        .as_ref()
        .expect("tool calls should be present");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call_1");
}

#[test]
fn inject_confirmation_token_adds_replay_token_without_clobbering_existing_value() {
    let injected = inject_confirmation_token(
        &serde_json::json!({"operation":"delete"}),
        Some("approval-1"),
    );
    assert_eq!(injected["confirmation_token"], "approval-1");

    let preserved = inject_confirmation_token(
        &serde_json::json!({"operation":"delete","confirmation_token":"existing"}),
        Some("approval-2"),
    );
    assert_eq!(preserved["confirmation_token"], "existing");
}

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo the input payload"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            }
        })
    }

    async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
        Ok(ToolOutput::success(input))
    }
}

struct PendingApprovalTool;

#[async_trait]
impl Tool for PendingApprovalTool {
    fn name(&self) -> &str {
        "approval_tool"
    }

    fn description(&self) -> &str {
        "Always returns pending approval"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            }
        })
    }

    async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
        Ok(ToolOutput {
            success: false,
            result: serde_json::json!({
                "pending_approval": true,
                "approval_id": "approval-test-1"
            }),
            error: Some("Approval required".to_string()),
            error_category: None,
            retryable: None,
            retry_after_ms: None,
        })
    }
}

struct RetryThenSuccessTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for RetryThenSuccessTool {
    fn name(&self) -> &str {
        "retry_once_tool"
    }

    fn description(&self) -> &str {
        "Fails once with retryable error then succeeds"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type":"object"})
    }

    async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
        let current = self.calls.fetch_add(1, Ordering::SeqCst);
        if current == 0 {
            Ok(ToolOutput::retryable_error(
                "temporary network failure",
                ToolErrorCategory::Network,
            ))
        } else {
            Ok(ToolOutput::success(serde_json::json!({"ok": true})))
        }
    }
}

struct NonRetryableTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for NonRetryableTool {
    fn name(&self) -> &str {
        "non_retryable_tool"
    }

    fn description(&self) -> &str {
        "Always fails with non-retryable config error"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type":"object"})
    }

    async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::non_retryable_error(
            "missing required config",
            ToolErrorCategory::Config,
        ))
    }
}

type ToolStartRecord = (String, String, String);
type ToolResultRecord = (String, String, String, bool);
type LlmCallRecord = (
    String,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<f64>,
    Option<u64>,
    Option<bool>,
    Option<u32>,
);
type ModelSwitchRecord = (String, String, Option<String>);

struct CapturingEmitter {
    text: Arc<AsyncMutex<Vec<String>>>,
    tool_starts: Arc<AsyncMutex<Vec<ToolStartRecord>>>,
    tool_results: Arc<AsyncMutex<Vec<ToolResultRecord>>>,
    completed: Arc<AtomicUsize>,
}

impl CapturingEmitter {
    fn new() -> Self {
        Self {
            text: Arc::new(AsyncMutex::new(Vec::new())),
            tool_starts: Arc::new(AsyncMutex::new(Vec::new())),
            tool_results: Arc::new(AsyncMutex::new(Vec::new())),
            completed: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[derive(Clone, Default)]
struct CapturingTelemetrySink {
    llm_calls: Arc<AsyncMutex<Vec<LlmCallRecord>>>,
    model_switches: Arc<AsyncMutex<Vec<ModelSwitchRecord>>>,
}

#[async_trait]
impl TelemetrySink for CapturingTelemetrySink {
    async fn emit(&self, event: ExecutionEventEnvelope) {
        match event.event {
            ExecutionEvent::LlmCall(trace) => {
                self.llm_calls.lock().await.push((
                    trace.model,
                    trace.input_tokens,
                    trace.output_tokens,
                    trace.total_tokens,
                    trace.cost_usd,
                    trace.duration_ms,
                    trace.is_reasoning,
                    trace.message_count,
                ));
            }
            ExecutionEvent::ModelSwitch {
                from_model,
                to_model,
                reason,
                ..
            } => {
                self.model_switches
                    .lock()
                    .await
                    .push((from_model, to_model, reason));
            }
            _ => {}
        }
    }
}

fn telemetry_context(model: &str) -> TelemetryContext {
    TelemetryContext::new(restflow_telemetry::RestflowTrace::new(
        "run-test",
        "session-test",
        "scope-test",
        "agent-test",
    ))
    .with_requested_model(model)
    .with_effective_model(model)
    .with_provider("openai")
}

#[async_trait]
impl StreamEmitter for CapturingEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        self.text.lock().await.push(text.to_string());
    }

    async fn emit_thinking_delta(&mut self, _text: &str) {}

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.tool_starts.lock().await.push((
            id.to_string(),
            name.to_string(),
            arguments.to_string(),
        ));
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        self.tool_results.lock().await.push((
            id.to_string(),
            name.to_string(),
            result.to_string(),
            success,
        ));
    }

    async fn emit_complete(&mut self) {
        self.completed.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_executor_simple_completion() {
    let response = CompletionResponse {
        content: Some("Hello, I'm done!".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: Some(TokenUsage {
            prompt_tokens: 20,
            completion_tokens: 10,
            total_tokens: 30,
            cost_usd: None,
        }),
    };

    let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(mock_llm.clone(), tools);

    let config = AgentConfig::new("Say hello");
    let result = executor.run(config).await.unwrap();

    assert!(result.success);
    assert_eq!(result.answer, Some("Hello, I'm done!".to_string()));
    assert_eq!(mock_llm.call_count(), 1);
}

#[tokio::test]
async fn test_execute_from_state_resumes_without_reinjecting_prompt() {
    let response = CompletionResponse {
        content: Some("Resumed done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(mock_llm.clone(), tools);

    let mut state = AgentState::new("resume-exec-1".to_string(), 10);
    state.iteration = 3;
    state.add_message(Message::system("Existing system"));
    state.add_message(Message::user("Existing user"));
    state.add_message(Message::assistant("Existing assistant"));

    let mut emitter = NullEmitter;
    let result = executor
        .execute_from_state(AgentConfig::new("ignored new goal"), state, &mut emitter)
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.state.execution_id, "resume-exec-1");
    assert_eq!(mock_llm.call_count(), 1);
    assert!(
        result
            .state
            .messages
            .iter()
            .any(|msg| msg.content == "Resumed done")
    );
}

#[tokio::test]
async fn test_executor_applies_llm_timeout_when_configured() {
    let llm = Arc::new(DelayedLlmClient::new(std::time::Duration::from_millis(120)));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(llm, tools);

    let config = AgentConfig::new("Slow request")
        .with_llm_timeout(std::time::Duration::from_millis(20))
        .with_max_iterations(1);
    let error = executor
        .run(config)
        .await
        .expect_err("configured LLM timeout should fail fast");
    assert!(error.to_string().contains("LLM completion timed out"));
}

#[tokio::test]
async fn test_executor_allows_disabling_llm_timeout() {
    let llm = Arc::new(DelayedLlmClient::new(std::time::Duration::from_millis(60)));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(llm, tools);

    let config = AgentConfig::new("Slow but allowed")
        .without_llm_timeout()
        .with_max_iterations(1);
    let result = executor
        .run(config)
        .await
        .expect("disabled LLM timeout should allow delayed completion");
    assert!(result.success);
    assert_eq!(result.answer.as_deref(), Some("Delayed done"));
}

#[tokio::test]
async fn test_checkpoint_durability_per_turn_triggers_callback() {
    let responses = vec![
        CompletionResponse {
            content: Some("Tool".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({"message":"hello"}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("Done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    let mock_llm = Arc::new(MockLlmClient::new(responses));
    let mut registry = ToolRegistry::new();
    registry.register(EchoTool);
    let executor = AgentExecutor::new(mock_llm, Arc::new(registry));

    let checkpoint_count = Arc::new(AtomicUsize::new(0));
    let count_ref = checkpoint_count.clone();
    let config = AgentConfig::new("checkpoint")
        .with_checkpoint_durability(CheckpointDurability::PerTurn)
        .with_checkpoint_callback(move |_| {
            let count_ref = count_ref.clone();
            async move {
                count_ref.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

    let result = executor.run(config).await.unwrap();
    assert!(result.success);
    assert_eq!(checkpoint_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_executor_uses_working_memory() {
    // Create a response that completes immediately
    let response = CompletionResponse {
        content: Some("Done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(mock_llm.clone(), tools);

    let config = AgentConfig::new("Test task")
        .with_system_prompt("You are a test assistant")
        .with_prompt_flags(PromptFlags::new().without_workspace_context());

    let result = executor.run(config).await.unwrap();
    assert!(result.success);

    // Verify the messages sent to LLM
    let requests = mock_llm.captured_requests();
    assert_eq!(requests.len(), 1);

    let messages = &requests[0];
    assert_eq!(messages.len(), 2); // system + user
    assert_eq!(messages[0].role, Role::System);
    assert_eq!(messages[1].role, Role::User);
    assert!(messages[1].content.contains("Test task"));
}

#[tokio::test]
async fn test_executor_multi_turn_with_tool_calls() {
    // Create responses for a multi-turn conversation
    let responses = vec![
        // First response with tool call
        CompletionResponse {
            content: Some("Let me help".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "unknown_tool".to_string(),
                arguments: serde_json::json!({}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        // Second response (completion)
        CompletionResponse {
            content: Some("All done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let mock_llm = Arc::new(MockLlmClient::new(responses));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(mock_llm.clone(), tools);

    let config = AgentConfig::new("Multi-turn task")
        .with_prompt_flags(PromptFlags::new().without_workspace_context());

    let result = executor.run(config).await.unwrap();
    assert!(result.success);
    assert_eq!(mock_llm.call_count(), 2);

    // Second call should have all messages (within limit)
    let requests = mock_llm.captured_requests();
    let second_request = &requests[1];

    // Should have: system, user, assistant (with tool calls), tool result
    assert_eq!(second_request.len(), 4);
}

#[tokio::test]
async fn test_executor_state_tracks_full_history() {
    let responses = vec![
        CompletionResponse {
            content: Some("Step 1".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "test".to_string(),
                arguments: serde_json::json!({}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("Done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let mock_llm = Arc::new(MockLlmClient::new(responses));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(mock_llm, tools);

    let config =
        AgentConfig::new("Test").with_prompt_flags(PromptFlags::new().without_workspace_context());

    let result = executor.run(config).await.unwrap();

    // State should have full history
    // system + user + assistant(tool_call) + tool_result + assistant(final)
    assert_eq!(result.state.messages.len(), 5);
}

#[tokio::test]
async fn test_executor_injects_workspace_instructions_as_user_message() {
    let response = CompletionResponse {
        content: Some("Done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let mut executor = AgentExecutor::new(llm.clone(), tools);

    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("AGENTS.md"),
        "System-like instruction from AGENTS file.",
    )
    .expect("write AGENTS.md");

    executor.context_cache = Some(WorkspaceContextCache::new(
        ContextDiscoveryConfig {
            paths: vec!["AGENTS.md".into()],
            scan_directories: false,
            case_insensitive_dedup: true,
            max_total_size: 100_000,
            max_file_size: 50_000,
        },
        temp.path().to_path_buf(),
    ));
    executor.workspace_root = Some(temp.path().to_path_buf());

    let config = AgentConfig::new("primary user goal");
    let result = executor.run(config).await.unwrap();
    assert!(result.success);

    let requests = llm.captured_requests();
    assert_eq!(requests.len(), 1);
    let messages = &requests[0];

    assert_eq!(messages[0].role, Role::System);
    assert!(
        !messages[0]
            .content
            .contains("System-like instruction from AGENTS file.")
    );

    let injected = messages.iter().find(|message| {
        message.role == Role::User && message.content.starts_with("# AGENTS.md instructions for ")
    });
    let injected = injected.expect("workspace instructions should be injected as a user message");
    assert!(
        injected
            .content
            .contains("System-like instruction from AGENTS file.")
    );

    let goal = messages
        .iter()
        .rev()
        .find(|message| message.role == Role::User)
        .expect("missing user goal message");
    assert!(goal.content.contains("primary user goal"));
}

#[tokio::test]
async fn test_executor_does_not_discover_workspace_from_current_dir() {
    let _lock = cwd_lock().await;
    let llm = Arc::new(MockLlmClient::new(Vec::new()));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(llm.clone(), tools);

    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("AGENTS.md"),
        "Implicit workspace instruction.",
    )
    .unwrap();
    let _guard = CurrentDirGuard::set(temp.path());

    let workspace_message = executor.build_workspace_instruction_user_message().await;
    assert!(workspace_message.is_none());
}

#[tokio::test]
async fn test_executor_defers_approval_and_continues() {
    let responses = vec![
        CompletionResponse {
            content: Some("Need a tool".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "approval_tool".to_string(),
                arguments: serde_json::json!({"command": "danger"}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("continued".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let mock_llm = Arc::new(MockLlmClient::new(responses));
    let mut registry = ToolRegistry::new();
    registry.register(PendingApprovalTool);
    let executor = AgentExecutor::new(mock_llm.clone(), Arc::new(registry));

    let result = executor
        .run(AgentConfig::new("test deferred"))
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(mock_llm.call_count(), 2);
    assert!(result.state.messages.iter().any(|m| {
        m.content
            .contains("Deferred execution for tool 'approval_tool'")
    }));
}

#[tokio::test]
async fn test_executor_retries_retryable_tool_errors() {
    let responses = vec![
        CompletionResponse {
            content: Some("try tool".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "retry_once_tool".to_string(),
                arguments: serde_json::json!({}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let calls = Arc::new(AtomicUsize::new(0));
    let tool = RetryThenSuccessTool {
        calls: calls.clone(),
    };
    let mock_llm = Arc::new(MockLlmClient::new(responses));
    let mut registry = ToolRegistry::new();
    registry.register(tool);
    let executor = AgentExecutor::new(mock_llm, Arc::new(registry));

    let result = executor.run(AgentConfig::new("retry test")).await.unwrap();
    assert!(result.success);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_executor_skips_retry_for_non_retryable_errors() {
    let responses = vec![
        CompletionResponse {
            content: Some("try tool".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "non_retryable_tool".to_string(),
                arguments: serde_json::json!({}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let calls = Arc::new(AtomicUsize::new(0));
    let tool = NonRetryableTool {
        calls: calls.clone(),
    };
    let mock_llm = Arc::new(MockLlmClient::new(responses));
    let mut registry = ToolRegistry::new();
    registry.register(tool);
    let executor = AgentExecutor::new(mock_llm, Arc::new(registry));

    let result = executor
        .run(AgentConfig::new("non retry test"))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_run_stream_basic() {
    let response = CompletionResponse {
        content: Some("stream-finished".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let mock_llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let executor = Arc::new(AgentExecutor::new(mock_llm, tools));

    let mut stream = executor.run_stream(AgentConfig::new("Say hello"));
    let mut saw_text_delta = false;
    let mut saw_completed = false;

    while let Some(step) = stream.next().await {
        match step {
            ExecutionStep::TextDelta { content } => {
                saw_text_delta = true;
                assert_eq!(content, "stream-finished");
            }
            ExecutionStep::Completed { result } => {
                assert!(result.success);
                saw_completed = true;
                break;
            }
            ExecutionStep::Failed { error } => panic!("unexpected failure: {error}"),
            _ => {}
        }
    }

    assert!(saw_text_delta);
    assert!(saw_completed);
}

#[tokio::test]
async fn test_run_stream_with_tools() {
    let responses = vec![
        CompletionResponse {
            content: Some("Calling tool".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({ "message": "hello" }),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let mock_llm = Arc::new(MockLlmClient::with_streaming(responses, false));
    let mut registry = ToolRegistry::new();
    registry.register(EchoTool);
    let executor = Arc::new(AgentExecutor::new(mock_llm, Arc::new(registry)));

    let mut stream = executor.run_stream(AgentConfig::new("Run echo"));
    let mut saw_tool_start = false;
    let mut saw_tool_result = false;
    let mut saw_completed = false;

    while let Some(step) = stream.next().await {
        match step {
            ExecutionStep::ToolCallStart { name, .. } => {
                if name == "echo" {
                    saw_tool_start = true;
                }
            }
            ExecutionStep::ToolCallResult { name, success, .. } => {
                if name == "echo" {
                    saw_tool_result = true;
                    assert!(success);
                }
            }
            ExecutionStep::Completed { result } => {
                saw_completed = true;
                assert!(result.success);
                break;
            }
            ExecutionStep::Failed { error } => panic!("unexpected failure: {error}"),
            _ => {}
        }
    }

    assert!(saw_tool_start);
    assert!(saw_tool_result);
    assert!(saw_completed);
}

#[tokio::test]
async fn test_utf8_truncation_chinese_chars() {
    // Create a tool result containing Chinese characters at boundary
    let chinese_text = "这是一个包含中文字符的测试）。".repeat(200); // ~4000 bytes

    let response = CompletionResponse {
        content: Some("Calling tool".to_string()),
        tool_calls: vec![ToolCall {
            id: "call_1".to_string(),
            name: "test".to_string(),
            arguments: serde_json::json!({"result": chinese_text}),
        }],
        finish_reason: FinishReason::ToolCalls,
        usage: None,
    };

    let mock_llm = Arc::new(MockLlmClient::new(vec![
        response,
        CompletionResponse {
            content: Some("Done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ]));

    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(mock_llm, tools);

    // Set max_tool_result_length to a value that would split Chinese chars
    let config = AgentConfig::new("Test UTF-8 safety").with_max_tool_result_length(4000);

    // This should NOT panic even with Chinese characters at byte boundary
    let result = executor.run(config).await;
    assert!(result.is_ok(), "Should handle Chinese characters safely");
    assert!(result.unwrap().success);
}

#[tokio::test]
#[allow(deprecated)]
async fn test_run_via_stream_matches_run_direct() {
    let response = CompletionResponse {
        content: Some("Unified path".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let direct_llm = Arc::new(MockLlmClient::new(vec![response.clone()]));
    let streaming_llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());

    let direct_executor = AgentExecutor::new(direct_llm, tools.clone());
    let streaming_executor = AgentExecutor::new(streaming_llm, tools);
    let config = AgentConfig::new("match");

    let direct = direct_executor.run(config.clone()).await.unwrap();
    let mut emitter = CapturingEmitter::new();
    let streamed = streaming_executor
        .execute_streaming(config, &mut emitter)
        .await
        .unwrap();

    assert_eq!(direct.success, streamed.success);
    assert_eq!(direct.answer, streamed.answer);
    assert_eq!(direct.error, streamed.error);
    assert_eq!(direct.iterations, streamed.iterations);
}

#[tokio::test]
#[allow(deprecated)]
async fn test_backward_compat_execute_streaming_emits_complete() {
    let response = CompletionResponse {
        content: Some("done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(llm, tools);
    let mut emitter = CapturingEmitter::new();

    let result = executor
        .execute_streaming(AgentConfig::new("compat"), &mut emitter)
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(emitter.completed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_non_stream_run_with_emitter_emits_tool_events() {
    let responses = vec![
        CompletionResponse {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({"message":"hello"}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: Some(TokenUsage {
                prompt_tokens: 12,
                completion_tokens: 6,
                total_tokens: 18,
                cost_usd: Some(0.02),
            }),
        },
        CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: Some(TokenUsage {
                prompt_tokens: 8,
                completion_tokens: 4,
                total_tokens: 12,
                cost_usd: Some(0.01),
            }),
        },
    ];

    let llm = Arc::new(MockLlmClient::new(responses));
    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);
    let executor = AgentExecutor::new(llm, Arc::new(tools));
    let mut emitter = CapturingEmitter::new();
    let telemetry_sink = CapturingTelemetrySink::default();

    let result = executor
        .run_with_emitter(
            AgentConfig::new("non-stream trace")
                .with_telemetry_sink(Arc::new(telemetry_sink.clone()))
                .with_telemetry_context(telemetry_context("mock-model")),
            &mut emitter,
        )
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(emitter.completed.load(Ordering::SeqCst), 1);
    assert_eq!(emitter.tool_starts.lock().await.len(), 1);
    assert_eq!(emitter.tool_results.lock().await.len(), 1);
    assert_eq!(telemetry_sink.llm_calls.lock().await.len(), 2);
    let tool_result = emitter.tool_results.lock().await;
    assert!(tool_result[0].3);
}

#[tokio::test]
async fn test_non_stream_run_from_state_with_emitter_emits_tool_events() {
    let responses = vec![
        CompletionResponse {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({"message":"resume"}),
            }],
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        },
        CompletionResponse {
            content: Some("done".to_string()),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let llm = Arc::new(MockLlmClient::new(responses));
    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);
    let executor = AgentExecutor::new(llm, Arc::new(tools));
    let mut emitter = CapturingEmitter::new();
    let mut state = AgentState::new("resume-exec".to_string(), 8);
    state.add_message(Message::system("system"));
    state.add_message(Message::user("resume"));
    let telemetry_sink = CapturingTelemetrySink::default();

    let result = executor
        .run_from_state_with_emitter(
            AgentConfig::new("unused-goal")
                .with_telemetry_sink(Arc::new(telemetry_sink.clone()))
                .with_telemetry_context(telemetry_context("mock-model")),
            state,
            &mut emitter,
        )
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(emitter.completed.load(Ordering::SeqCst), 1);
    assert_eq!(emitter.tool_starts.lock().await.len(), 1);
    assert_eq!(emitter.tool_results.lock().await.len(), 1);
    assert_eq!(telemetry_sink.llm_calls.lock().await.len(), 2);
}

#[tokio::test]
async fn test_non_stream_run_with_emitter_records_llm_usage() {
    let response = CompletionResponse {
        content: Some("done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cost_usd: Some(0.03),
        }),
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let executor = AgentExecutor::new(llm, Arc::new(ToolRegistry::new()));
    let mut emitter = CapturingEmitter::new();
    let telemetry_sink = CapturingTelemetrySink::default();

    let result = executor
        .run_with_emitter(
            AgentConfig::new("non-stream llm trace")
                .with_telemetry_sink(Arc::new(telemetry_sink.clone()))
                .with_telemetry_context(telemetry_context("mock-model")),
            &mut emitter,
        )
        .await
        .unwrap();

    assert!(result.success);
    let llm_calls = telemetry_sink.llm_calls.lock().await;
    assert_eq!(llm_calls.len(), 1);
    assert_eq!(llm_calls[0].0, "mock-model");
    assert_eq!(llm_calls[0].1, Some(10));
    assert_eq!(llm_calls[0].2, Some(5));
    assert_eq!(llm_calls[0].3, Some(15));
    assert_eq!(llm_calls[0].4, Some(0.03));
    assert_eq!(llm_calls[0].6, None);
    assert!(llm_calls[0].5.is_some());
    assert!(llm_calls[0].7.unwrap_or_default() >= 2);
}

#[tokio::test]
async fn test_run_with_emitter_emits_model_switch_for_routing() {
    struct RecordingSwitcher {
        current: Mutex<String>,
    }

    impl restflow_traits::llm::LlmSwitcher for RecordingSwitcher {
        fn current_model(&self) -> String {
            self.current.lock().unwrap().clone()
        }

        fn current_provider(&self) -> String {
            "mock".to_string()
        }

        fn available_models(&self) -> Vec<String> {
            vec!["gpt-5".to_string(), "gpt-5-pro".to_string()]
        }

        fn provider_for_model(&self, _model: &str) -> Option<LlmProvider> {
            Some(LlmProvider::OpenAI)
        }

        fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
            Some("test-key".to_string())
        }

        fn client_kind_for_model(&self, _model: &str) -> Option<ClientKind> {
            Some(ClientKind::Http)
        }

        fn create_and_swap(
            &self,
            model: &str,
            _api_key: Option<&str>,
        ) -> std::result::Result<restflow_traits::llm::SwapResult, restflow_traits::ToolError>
        {
            let previous_model = self.current();
            *self.current.lock().unwrap() = model.to_string();
            Ok(restflow_traits::llm::SwapResult {
                previous_provider: "openai".to_string(),
                previous_model,
                previous_runtime_provider: Some(LlmProvider::OpenAI),
                new_provider: "openai".to_string(),
                new_model: model.to_string(),
                new_runtime_provider: LlmProvider::OpenAI,
            })
        }
    }

    impl RecordingSwitcher {
        fn current(&self) -> String {
            self.current.lock().unwrap().clone()
        }
    }

    let response = CompletionResponse {
        content: Some("done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let executor = AgentExecutor::new(llm, Arc::new(ToolRegistry::new()));
    let switcher = Arc::new(RecordingSwitcher {
        current: Mutex::new("gpt-5".to_string()),
    });
    let mut emitter = CapturingEmitter::new();
    let telemetry_sink = CapturingTelemetrySink::default();

    let result = executor
        .run_with_emitter(
            AgentConfig::new("list files and check status")
                .with_model_routing(crate::agent::ModelRoutingConfig {
                    enabled: true,
                    routine_model: Some("gpt-5.4-mini".to_string()),
                    moderate_model: None,
                    complex_model: None,
                    escalate_on_failure: true,
                })
                .with_model_switcher(switcher)
                .with_telemetry_sink(Arc::new(telemetry_sink.clone()))
                .with_telemetry_context(telemetry_context("gpt-5")),
            &mut emitter,
        )
        .await
        .unwrap();

    assert!(result.success);
    let switches = telemetry_sink.model_switches.lock().await;
    assert_eq!(switches.len(), 1);
    assert_eq!(
        switches[0],
        (
            "gpt-5".to_string(),
            "gpt-5.4-mini".to_string(),
            Some("routing".to_string())
        )
    );
}

#[test]
fn test_parse_approval_resolution() {
    assert_eq!(
        parse_approval_resolution("approval abc approved"),
        Some(("abc".to_string(), true, None))
    );
    assert_eq!(
        parse_approval_resolution("approval id-1 denied too dangerous"),
        Some(("id-1".to_string(), false, Some("too dangerous".to_string())))
    );
    assert!(parse_approval_resolution("hello world").is_none());
}

#[tokio::test]
async fn test_prompt_flags_disable_tools() {
    let response = CompletionResponse {
        content: Some("Done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    // Disable tools section
    let flags = PromptFlags::new().without_tools();
    let config = AgentConfig::new("test").with_prompt_flags(flags);

    let prompt = executor.build_system_prompt(&config).await;

    // Should NOT contain tools section
    assert!(!prompt.contains("Available Tools"));
    // Should contain base section
    assert!(prompt.contains("helpful AI assistant"));
}

#[tokio::test]
async fn test_prompt_flags_disable_base() {
    let response = CompletionResponse {
        content: Some("Done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let tools = Arc::new(ToolRegistry::new());
    let executor = AgentExecutor::new(llm, tools);

    // Disable base section
    let flags = PromptFlags::new().without_base();
    let config = AgentConfig::new("test").with_prompt_flags(flags);

    let prompt = executor.build_system_prompt(&config).await;

    // Should NOT contain base prompt
    assert!(!prompt.contains("helpful AI assistant"));
    // Should be empty or minimal
    assert!(prompt.is_empty() || prompt.len() < 20);
}

#[tokio::test]
async fn test_prompt_flags_default_all_enabled() {
    let response = CompletionResponse {
        content: Some("Done".to_string()),
        tool_calls: vec![],
        finish_reason: FinishReason::Stop,
        usage: None,
    };

    let llm = Arc::new(MockLlmClient::new(vec![response]));
    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    // Default flags should enable all sections
    let config = AgentConfig::new("test");

    let prompt = executor.build_system_prompt(&config).await;

    // Should contain all sections
    assert!(prompt.contains("helpful AI assistant"));
    assert!(prompt.contains("Available Tools"));
    assert!(prompt.contains("echo"));
}

// ── Parallel execution tests ──

/// A tool that sleeps for a configurable duration then returns its name.
/// Used to verify ordering and true parallelism.
struct DelayTool {
    tool_name: String,
    delay_ms: u64,
}

#[async_trait]
impl Tool for DelayTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        "Sleeps then returns its name"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        Ok(ToolOutput::success(
            serde_json::json!({"tool": self.tool_name}),
        ))
    }
}

/// A tool that panics inside execute.
struct PanicTool;

#[async_trait]
impl Tool for PanicTool {
    fn name(&self) -> &str {
        "panic_tool"
    }

    fn description(&self) -> &str {
        "Always panics"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
        panic!("intentional panic for testing");
    }
}

/// A tool that sleeps forever (for timeout testing).
struct HangTool;

#[async_trait]
impl Tool for HangTool {
    fn name(&self) -> &str {
        "hang_tool"
    }

    fn description(&self) -> &str {
        "Sleeps forever"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
        // Sleep long enough that the timeout will fire
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Ok(ToolOutput::success(serde_json::json!({})))
    }
}

/// A spawn_subagent-shaped tool that returns input as output so tests can verify argument injection.
struct SpawnSubagentCaptureTool;

#[async_trait]
impl Tool for SpawnSubagentCaptureTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Capture spawn_subagent input payload"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
        Ok(ToolOutput::success(input))
    }
}

/// A spawn_subagent_batch-shaped tool that returns input as output so tests can
/// verify runtime-owned parent/trace injection.
struct SpawnSubagentBatchCaptureTool;

#[async_trait]
impl Tool for SpawnSubagentBatchCaptureTool {
    fn name(&self) -> &str {
        "spawn_subagent_batch"
    }

    fn description(&self) -> &str {
        "Capture spawn_subagent_batch input payload"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
        Ok(ToolOutput::success(input))
    }
}

/// A manage_background_agents-shaped tool that returns input as output so tests
/// can verify session_id injection for promote_to_background.
struct PromoteBackgroundCaptureTool;

#[async_trait]
impl Tool for PromoteBackgroundCaptureTool {
    fn name(&self) -> &str {
        "manage_background_agents"
    }

    fn description(&self) -> &str {
        "Capture manage_background_agents input payload"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
        Ok(ToolOutput::success(input))
    }
}

struct SubagentReadCaptureTool {
    tool_name: &'static str,
}

#[async_trait]
impl Tool for SubagentReadCaptureTool {
    fn name(&self) -> &str {
        self.tool_name
    }

    fn description(&self) -> &str {
        "Capture subagent read input payload"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
        Ok(ToolOutput::success(input))
    }
}

struct ToolStartCaptureEmitter {
    start_arguments: Arc<AsyncMutex<Vec<String>>>,
}

impl ToolStartCaptureEmitter {
    fn new() -> Self {
        Self {
            start_arguments: Arc::new(AsyncMutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl StreamEmitter for ToolStartCaptureEmitter {
    async fn emit_text_delta(&mut self, _text: &str) {}

    async fn emit_thinking_delta(&mut self, _text: &str) {}

    async fn emit_tool_call_start(&mut self, _id: &str, _name: &str, arguments: &str) {
        self.start_arguments
            .lock()
            .await
            .push(arguments.to_string());
    }

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

#[tokio::test]
async fn test_parallel_tools_returns_results_in_submission_order() {
    // Tool A sleeps 100ms, Tool B sleeps 10ms, Tool C sleeps 50ms.
    // Despite different completion times, results must come back in A, B, C order.
    let mut tools = ToolRegistry::new();
    tools.register(DelayTool {
        tool_name: "tool_a".to_string(),
        delay_ms: 100,
    });
    tools.register(DelayTool {
        tool_name: "tool_b".to_string(),
        delay_ms: 10,
    });
    tools.register(DelayTool {
        tool_name: "tool_c".to_string(),
        delay_ms: 50,
    });

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![
        ToolCall {
            id: "call_a".to_string(),
            name: "tool_a".to_string(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "call_b".to_string(),
            name: "tool_b".to_string(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "call_c".to_string(),
            name: "tool_c".to_string(),
            arguments: serde_json::json!({}),
        },
    ];

    let mut emitter = NullEmitter;
    let timeout = Duration::from_secs(10);
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: timeout,
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext::default(),
            },
        )
        .await;

    // Verify submission order preserved
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, "call_a");
    assert_eq!(results[1].0, "call_b");
    assert_eq!(results[2].0, "call_c");

    // Verify all succeeded
    for (id, result) in &results {
        let output = result
            .as_ref()
            .unwrap_or_else(|e| panic!("{id} failed: {e}"));
        assert!(output.success, "{id} should succeed");
    }
}

#[tokio::test]
async fn test_parallel_tools_true_concurrency() {
    // Two tools each sleep 50ms. If truly parallel, total time should be
    // well under 100ms (the sequential sum). We allow generous headroom.
    let mut tools = ToolRegistry::new();
    tools.register(DelayTool {
        tool_name: "slow_a".to_string(),
        delay_ms: 50,
    });
    tools.register(DelayTool {
        tool_name: "slow_b".to_string(),
        delay_ms: 50,
    });

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![
        ToolCall {
            id: "a".to_string(),
            name: "slow_a".to_string(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "b".to_string(),
            name: "slow_b".to_string(),
            arguments: serde_json::json!({}),
        },
    ];

    let mut emitter = NullEmitter;
    let start = std::time::Instant::now();
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(10),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext::default(),
            },
        )
        .await;
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 2);
    // If sequential, would take >= 100ms. Parallel should be ~50ms.
    assert!(
        elapsed < Duration::from_millis(90),
        "Expected parallel execution under 90ms, took {:?}",
        elapsed,
    );
}

#[tokio::test]
async fn test_parallel_tools_panic_recovery() {
    // One tool panics, other succeeds. The panic should be captured
    // without crashing the executor.
    let mut tools = ToolRegistry::new();
    tools.register(PanicTool);
    tools.register(DelayTool {
        tool_name: "good_tool".to_string(),
        delay_ms: 10,
    });

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![
        ToolCall {
            id: "panic_call".to_string(),
            name: "panic_tool".to_string(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "good_call".to_string(),
            name: "good_tool".to_string(),
            arguments: serde_json::json!({}),
        },
    ];

    let mut emitter = NullEmitter;
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(10),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext::default(),
            },
        )
        .await;

    assert_eq!(results.len(), 2);

    // Panicked tool should return an error containing "panicked"
    let (id, result) = &results[0];
    assert_eq!(id, "panic_call");
    assert!(result.is_err());
    let err_msg = format!("{}", result.as_ref().unwrap_err());
    assert!(
        err_msg.contains("panicked"),
        "Expected panic error, got: {err_msg}",
    );

    // Good tool should succeed normally
    let (id, result) = &results[1];
    assert_eq!(id, "good_call");
    assert!(result.is_ok());
    assert!(result.as_ref().unwrap().success);
}

#[tokio::test]
async fn test_parallel_tools_timeout_in_spawned_task() {
    // A hanging tool should be caught by the timeout inside the spawned task.
    let mut tools = ToolRegistry::new();
    tools.register(HangTool);
    tools.register(DelayTool {
        tool_name: "fast_tool".to_string(),
        delay_ms: 10,
    });

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![
        ToolCall {
            id: "hang_call".to_string(),
            name: "hang_tool".to_string(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "fast_call".to_string(),
            name: "fast_tool".to_string(),
            arguments: serde_json::json!({}),
        },
    ];

    let mut emitter = NullEmitter;
    // Short timeout to trigger quickly
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_millis(200),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext::default(),
            },
        )
        .await;

    assert_eq!(results.len(), 2);

    // Hanging tool should error with timeout message
    let (id, result) = &results[0];
    assert_eq!(id, "hang_call");
    assert!(result.is_err());
    let err_msg = format!("{}", result.as_ref().unwrap_err());
    assert!(
        err_msg.contains("timed out"),
        "Expected timeout error, got: {err_msg}",
    );

    // Fast tool should succeed despite the other timing out
    let (id, result) = &results[1];
    assert_eq!(id, "fast_call");
    assert!(result.is_ok());
    assert!(result.as_ref().unwrap().success);
}

#[tokio::test]
async fn test_spawn_subagent_tool_call_injects_parent_execution_id() {
    let mut tools = ToolRegistry::new();
    tools.register(SpawnSubagentCaptureTool);

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![ToolCall {
        id: "spawn_call".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: serde_json::json!({
            "agent": "default",
            "task": "Investigate"
        }),
    }];

    let mut emitter = ToolStartCaptureEmitter::new();
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: Some("exec-parent-1"),
                    chat_session_id: None,
                    trace_session_id: Some("session-main-1"),
                    trace_scope_id: Some("scope-main-1"),
                },
            },
        )
        .await;

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("spawn_call should succeed: {e}"));
    assert_eq!(output.result["parent_execution_id"], "exec-parent-1");
    assert_eq!(output.result["trace_session_id"], "session-main-1");
    assert_eq!(output.result["trace_scope_id"], "scope-main-1");

    let start_arguments = emitter.start_arguments.lock().await;
    assert_eq!(start_arguments.len(), 1);
    let start_payload: Value = serde_json::from_str(&start_arguments[0]).expect("valid json");
    assert_eq!(start_payload["parent_execution_id"], "exec-parent-1");
    assert_eq!(start_payload["trace_session_id"], "session-main-1");
    assert_eq!(start_payload["trace_scope_id"], "scope-main-1");
}

#[tokio::test]
async fn test_spawn_subagent_tool_call_overrides_explicit_parent_execution_id() {
    let mut tools = ToolRegistry::new();
    tools.register(SpawnSubagentCaptureTool);

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![ToolCall {
        id: "spawn_call".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: serde_json::json!({
            "agent": "default",
            "task": "Investigate",
            "parent_execution_id": "explicit-parent"
        }),
    }];

    let mut emitter = NullEmitter;
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: Some("runtime-parent"),
                    chat_session_id: None,
                    trace_session_id: Some("runtime-session"),
                    trace_scope_id: Some("runtime-scope"),
                },
            },
        )
        .await;

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("spawn_call should succeed: {e}"));
    assert_eq!(output.result["parent_execution_id"], "runtime-parent");
    assert_eq!(output.result["trace_session_id"], "runtime-session");
    assert_eq!(output.result["trace_scope_id"], "runtime-scope");
}

#[tokio::test]
async fn test_spawn_subagent_tool_call_overrides_explicit_trace_context() {
    let mut tools = ToolRegistry::new();
    tools.register(SpawnSubagentCaptureTool);

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![ToolCall {
        id: "spawn_call".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: serde_json::json!({
            "agent": "default",
            "task": "Investigate",
            "trace_session_id": "explicit-session",
            "trace_scope_id": "explicit-scope",
            "parent_execution_id": "explicit-parent"
        }),
    }];

    let mut emitter = NullEmitter;
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: Some("runtime-parent"),
                    chat_session_id: None,
                    trace_session_id: Some("runtime-session"),
                    trace_scope_id: Some("runtime-scope"),
                },
            },
        )
        .await;

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("spawn_call should succeed: {e}"));
    assert_eq!(output.result["parent_execution_id"], "runtime-parent");
    assert_eq!(output.result["trace_session_id"], "runtime-session");
    assert_eq!(output.result["trace_scope_id"], "runtime-scope");
}

#[tokio::test]
async fn test_spawn_subagent_batch_overrides_explicit_parent_and_trace_context() {
    let mut tools = ToolRegistry::new();
    tools.register(SpawnSubagentBatchCaptureTool);

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![ToolCall {
        id: "spawn_batch_call".to_string(),
        name: "spawn_subagent_batch".to_string(),
        arguments: serde_json::json!({
            "operation": "spawn",
            "specs": [
                {
                    "agent": "default",
                    "task": "Investigate"
                }
            ],
            "parent_execution_id": "explicit-parent",
            "trace_session_id": "explicit-session",
            "trace_scope_id": "explicit-scope"
        }),
    }];

    let mut emitter = ToolStartCaptureEmitter::new();
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: Some("runtime-parent"),
                    chat_session_id: None,
                    trace_session_id: Some("runtime-session"),
                    trace_scope_id: Some("runtime-scope"),
                },
            },
        )
        .await;

    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("spawn_batch_call should succeed: {e}"));
    assert_eq!(output.result["parent_execution_id"], "runtime-parent");
    assert_eq!(output.result["trace_session_id"], "runtime-session");
    assert_eq!(output.result["trace_scope_id"], "runtime-scope");

    let start_arguments = emitter.start_arguments.lock().await;
    let start_payload: Value = serde_json::from_str(&start_arguments[0]).expect("valid json");
    assert_eq!(start_payload["parent_execution_id"], "runtime-parent");
    assert_eq!(start_payload["trace_session_id"], "runtime-session");
    assert_eq!(start_payload["trace_scope_id"], "runtime-scope");
}

#[tokio::test]
async fn test_promote_to_background_injects_chat_session_id() {
    let mut tools = ToolRegistry::new();
    tools.register(PromoteBackgroundCaptureTool);

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![ToolCall {
        id: "promote_call".to_string(),
        name: "manage_background_agents".to_string(),
        arguments: serde_json::json!({
            "operation": "promote_to_background",
            "name": "Promoted Task"
        }),
    }];

    let mut emitter = ToolStartCaptureEmitter::new();
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: None,
                    chat_session_id: Some("session-main-1"),
                    trace_session_id: None,
                    trace_scope_id: None,
                },
            },
        )
        .await;

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("promote_call should succeed: {e}"));
    assert_eq!(output.result["session_id"], "session-main-1");

    let start_arguments = emitter.start_arguments.lock().await;
    assert_eq!(start_arguments.len(), 1);
    let start_payload: Value = serde_json::from_str(&start_arguments[0]).expect("valid json");
    assert_eq!(start_payload["session_id"], "session-main-1");
}

#[tokio::test]
async fn test_promote_to_background_overrides_explicit_session_id() {
    let mut tools = ToolRegistry::new();
    tools.register(PromoteBackgroundCaptureTool);

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));

    let calls = vec![ToolCall {
        id: "promote_call".to_string(),
        name: "manage_background_agents".to_string(),
        arguments: serde_json::json!({
            "operation": "promote_to_background",
            "session_id": "session-explicit",
            "name": "Promoted Task"
        }),
    }];

    let mut emitter = NullEmitter;
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: None,
                    chat_session_id: Some("session-main-1"),
                    trace_session_id: None,
                    trace_scope_id: None,
                },
            },
        )
        .await;

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("promote_call should succeed: {e}"));
    assert_eq!(output.result["session_id"], "session-main-1");
}

#[tokio::test]
async fn test_list_subagents_injects_parent_run_id() {
    let mut tools = ToolRegistry::new();
    tools.register(SubagentReadCaptureTool {
        tool_name: "list_subagents",
    });

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));
    let calls = vec![ToolCall {
        id: "list_call".to_string(),
        name: "list_subagents".to_string(),
        arguments: serde_json::json!({}),
    }];

    let mut emitter = ToolStartCaptureEmitter::new();
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: Some("parent-run-1"),
                    chat_session_id: None,
                    trace_session_id: None,
                    trace_scope_id: None,
                },
            },
        )
        .await;

    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("list_call should succeed: {e}"));
    assert_eq!(output.result["parent_run_id"], "parent-run-1");

    let start_arguments = emitter.start_arguments.lock().await;
    let start_payload: Value = serde_json::from_str(&start_arguments[0]).expect("valid json");
    assert_eq!(start_payload["parent_run_id"], "parent-run-1");
}

#[tokio::test]
async fn test_wait_subagents_overrides_explicit_parent_run_id() {
    let mut tools = ToolRegistry::new();
    tools.register(SubagentReadCaptureTool {
        tool_name: "wait_subagents",
    });

    let llm = Arc::new(MockLlmClient::new(vec![]));
    let executor = AgentExecutor::new(llm, Arc::new(tools));
    let calls = vec![ToolCall {
        id: "wait_call".to_string(),
        name: "wait_subagents".to_string(),
        arguments: serde_json::json!({
            "task_ids": ["child-1"],
            "parent_run_id": "explicit-parent"
        }),
    }];

    let mut emitter = NullEmitter;
    let results = executor
        .execute_tools_parallel(
            &calls,
            &mut emitter,
            ToolExecutionOptions {
                tool_timeout: Duration::from_secs(5),
                yolo_mode: false,
                max_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
                telemetry_sink: None,
                telemetry_context: None,
                invocation: ToolInvocationContext {
                    parent_execution_id: Some("runtime-parent"),
                    chat_session_id: None,
                    trace_session_id: None,
                    trace_scope_id: None,
                },
            },
        )
        .await;

    let (_, result) = &results[0];
    let output = result
        .as_ref()
        .unwrap_or_else(|e| panic!("wait_call should succeed: {e}"));
    assert_eq!(output.result["parent_run_id"], "runtime-parent");
}

#[test]
fn test_truncate_tool_output_short_content_unchanged() {
    let short = "hello world";
    let result = truncate_tool_output(short, 100, None, "c1", "bash");
    assert_eq!(result, short);
}

#[test]
fn test_truncate_tool_output_middle_truncation_without_output_dir() {
    let long = "a".repeat(500);
    let result = truncate_tool_output(&long, 100, None, "c1", "bash");
    // Should contain the middle-truncation marker
    assert!(result.contains("chars truncated"));
    // Should not contain file hint (no output dir configured)
    assert!(!result.contains("saved to"));
    assert!(result.len() <= 100);
}

#[test]
fn test_truncate_tool_output_with_tool_output_dir_saves_and_hints() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("tool-output");

    let long = "x".repeat(1000);
    let result = truncate_tool_output(&long, 200, Some(output_dir.as_path()), "call-7", "bash");

    // Should contain the retrieval hint
    assert!(result.contains("Full output (1000 chars) saved to:"));
    assert!(result.contains("bash-call-7.txt"));

    // Verify the file was actually created with full content
    let saved = std::fs::read_to_string(output_dir.join("bash-call-7.txt")).unwrap();
    assert_eq!(saved.len(), 1000);
}

#[test]
fn test_truncate_tool_output_exact_boundary() {
    let exact = "b".repeat(100);
    let result = truncate_tool_output(&exact, 100, None, "c1", "test");
    assert_eq!(result, exact);
}
