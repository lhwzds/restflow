use std::sync::Arc;

use serde_json::json;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};

use crate::agent::PromptFlags;
use crate::agent::executor::{AgentConfig, AgentExecutor, AgentResult};
use crate::agent::stream::StreamEmitter;
use crate::error::{AiError, Result};
use crate::llm::{LlmClient, LlmClientFactory};
use crate::tools::{FilteredToolset, ToolRegistry};
use restflow_traits::Toolset;

use super::model_resolution::resolve_llm_client;
use super::trace::{RunTraceContext, RunTraceOutcome};
use super::tracker::SubagentTracker;

pub use restflow_traits::SubagentConfig;
pub use restflow_traits::subagent::{
    InlineSubagentConfig, SpawnHandle, SpawnRequest, SubagentDefLookup, SubagentDefSnapshot,
};

const TEMPORARY_SUBAGENT_NAME: &str = "Temporary Subagent";
const TEMPORARY_SUBAGENT_PROMPT: &str = "You are a temporary sub-agent. Complete the task autonomously, use tools when needed, and return a concise final result.";

fn map_subagent_error(success: bool, error: Option<String>) -> Option<String> {
    if success {
        None
    } else {
        error.or_else(|| Some("Sub-agent execution failed".to_string()))
    }
}

/// Spawn a sub-agent with the given request.
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    request: SpawnRequest,
    llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
) -> Result<SpawnHandle> {
    let agent_def = resolve_subagent_definition(&definitions, &tool_registry, &request)?;

    let llm_client = resolve_llm_client(
        request.model.as_deref(),
        request.model_provider.as_deref(),
        agent_def.default_model.as_deref(),
        &llm_client,
        llm_client_factory.as_ref(),
    )?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = request.timeout_secs.unwrap_or(config.subagent_timeout_secs);

    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();
    let parent_execution_id = request.parent_execution_id.clone();
    let trace_context = RunTraceContext {
        run_id: task_id.clone(),
        actor_id: agent_name_for_return.clone(),
        parent_run_id: parent_execution_id.clone(),
    };
    let trace_lifecycle_sink = tracker.trace_lifecycle_sink();
    let trace_emitter_factory = tracker.trace_emitter_factory();
    let max_parallel = config.max_parallel_agents;

    tracker.try_reserve(
        max_parallel,
        task_id.clone(),
        agent_name_for_register,
        task_for_register,
    )?;

    if let Some(sink) = trace_lifecycle_sink.as_ref() {
        sink.on_run_started(&trace_context);
    }

    let task = request.task.clone();
    let tracker_clone = tracker.clone();
    let task_id_for_spawn = task_id.clone();
    let llm_client = llm_client.clone();
    let tool_registry = tool_registry.clone();
    let config_clone = config.clone();
    let trace_context_for_spawn = trace_context.clone();
    let trace_lifecycle_sink_for_spawn = trace_lifecycle_sink.clone();
    let trace_emitter_factory_for_spawn = trace_emitter_factory.clone();

    let (completion_tx, completion_rx) = oneshot::channel();
    let (start_tx, start_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let task_id = task_id_for_spawn;
        if start_rx.await.is_err() {
            return restflow_traits::SubagentResult {
                success: false,
                output: String::new(),
                summary: None,
                duration_ms: 0,
                tokens_used: None,
                cost_usd: None,
                error: Some("Sub-agent registration cancelled".to_string()),
            };
        }
        let start = std::time::Instant::now();
        let mut trace_emitter = trace_emitter_factory_for_spawn
            .as_ref()
            .map(|factory| factory.build_run_emitter(&trace_context_for_spawn));

        let result = if let Some(emitter) = trace_emitter.as_mut() {
            timeout(
                Duration::from_secs(timeout_secs),
                execute_subagent(
                    llm_client,
                    tool_registry,
                    agent_def,
                    task.clone(),
                    config_clone,
                    parent_execution_id.clone(),
                    Some(emitter.as_mut()),
                ),
            )
            .await
        } else {
            timeout(
                Duration::from_secs(timeout_secs),
                execute_subagent(
                    llm_client,
                    tool_registry,
                    agent_def,
                    task.clone(),
                    config_clone,
                    parent_execution_id.clone(),
                    None,
                ),
            )
            .await
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let (subagent_result, timed_out) = match result {
            Ok(Ok(result)) => {
                let AgentResult {
                    success,
                    answer,
                    error,
                    total_tokens,
                    total_cost_usd,
                    ..
                } = result;
                let cost_usd = if total_cost_usd > 0.0 {
                    Some(total_cost_usd)
                } else {
                    None
                };
                (
                    restflow_traits::SubagentResult {
                        success,
                        output: answer.unwrap_or_default(),
                        summary: None,
                        duration_ms,
                        tokens_used: Some(total_tokens),
                        cost_usd,
                        error: map_subagent_error(success, error),
                    },
                    false,
                )
            }
            Ok(Err(error)) => (
                restflow_traits::SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some(error.to_string()),
                },
                false,
            ),
            Err(_) => (
                restflow_traits::SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some("Sub-agent timed out".to_string()),
                },
                true,
            ),
        };

        if timed_out {
            tracker_clone.mark_timed_out_with_result(&task_id, subagent_result.clone());
        } else {
            tracker_clone.mark_completed(&task_id, subagent_result.clone());
        }

        if let Some(sink) = trace_lifecycle_sink_for_spawn.as_ref() {
            sink.on_run_finished(
                &trace_context_for_spawn,
                &RunTraceOutcome {
                    success: subagent_result.success,
                    error: subagent_result.error.clone(),
                },
            );
        }

        let _ = completion_tx.send(subagent_result.clone());
        subagent_result
    });

    if let Err(error) = tracker.attach_execution(task_id.clone(), handle, completion_rx) {
        let failure = restflow_traits::SubagentResult {
            success: false,
            output: String::new(),
            summary: None,
            duration_ms: 0,
            tokens_used: None,
            cost_usd: None,
            error: Some(error.to_string()),
        };
        if let Some(sink) = trace_lifecycle_sink.as_ref() {
            sink.on_run_finished(
                &trace_context,
                &RunTraceOutcome {
                    success: failure.success,
                    error: failure.error.clone(),
                },
            );
        }
        tracker.mark_completed(&task_id, failure);
        return Err(error);
    }

    let _ = start_tx.send(());

    Ok(SpawnHandle {
        id: task_id,
        agent_name: agent_name_for_return,
    })
}

fn resolve_subagent_definition(
    definitions: &Arc<dyn SubagentDefLookup>,
    tool_registry: &Arc<ToolRegistry>,
    request: &SpawnRequest,
) -> Result<SubagentDefSnapshot> {
    if let Some(agent_id) = request
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return definitions
            .lookup(agent_id)
            .ok_or_else(|| AiError::Agent(format!("Unknown agent type: {agent_id}")));
    }

    Ok(build_temporary_subagent_definition(
        request.inline.as_ref(),
        tool_registry,
    ))
}

fn build_temporary_subagent_definition(
    inline: Option<&InlineSubagentConfig>,
    tool_registry: &Arc<ToolRegistry>,
) -> SubagentDefSnapshot {
    let fallback_tools = tool_registry
        .list()
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let name = inline
        .and_then(|cfg| cfg.name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(TEMPORARY_SUBAGENT_NAME)
        .to_string();
    let system_prompt = inline
        .and_then(|cfg| cfg.system_prompt.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(TEMPORARY_SUBAGENT_PROMPT)
        .to_string();
    let allowed_tools = inline
        .and_then(|cfg| cfg.allowed_tools.clone())
        .unwrap_or(fallback_tools);

    SubagentDefSnapshot {
        name,
        system_prompt,
        allowed_tools,
        max_iterations: inline
            .and_then(|cfg| cfg.max_iterations)
            .filter(|value| *value > 0),
        default_model: None,
    }
}

async fn execute_subagent(
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    agent_def: SubagentDefSnapshot,
    task: String,
    config: SubagentConfig,
    parent_execution_id: Option<String>,
    mut emitter: Option<&mut dyn StreamEmitter>,
) -> Result<AgentResult> {
    let registry = build_registry_for_agent(
        &tool_registry,
        &agent_def.allowed_tools,
        1,
        config.max_depth,
    );
    let registry = Arc::new(registry);

    let max_iterations = agent_def
        .max_iterations
        .map(|value| value as usize)
        .unwrap_or(config.max_iterations);

    let agent_config = build_subagent_agent_config(
        task.clone(),
        agent_def.system_prompt.clone(),
        max_iterations,
        parent_execution_id.as_deref(),
    );

    let executor = AgentExecutor::new(llm_client, registry);
    let result = if let Some(emitter) = emitter.as_mut() {
        executor.run_with_emitter(agent_config, *emitter).await?
    } else {
        executor.run(agent_config).await?
    };

    Ok(result)
}

fn build_subagent_agent_config(
    task: String,
    system_prompt: String,
    max_iterations: usize,
    parent_execution_id: Option<&str>,
) -> AgentConfig {
    let mut agent_config = AgentConfig::new(task);
    agent_config.system_prompt = Some(system_prompt);
    agent_config.max_iterations = max_iterations;
    agent_config.prompt_flags = PromptFlags::new().without_workspace_context();
    agent_config.yolo_mode = true;
    agent_config = agent_config.with_context(
        "execution_context",
        json!({
            "role": "subagent",
            "parent_execution_id": parent_execution_id,
        }),
    );
    agent_config = agent_config.with_context("execution_role", json!("subagent"));
    agent_config
}

fn build_registry_for_agent(
    parent: &Arc<ToolRegistry>,
    allowed_tools: &[String],
    current_depth: usize,
    max_depth: usize,
) -> ToolRegistry {
    let filtered = FilteredToolset::from_allowlist(parent.clone(), allowed_tools);
    let mut registry = ToolRegistry::new();

    const COLLAB_TOOLS: &[&str] = &[
        "spawn_subagent",
        "wait_subagents",
        "list_subagents",
        "cancel_agent",
        "send_input",
    ];
    let at_depth_limit = max_depth > 0 && current_depth >= max_depth;

    for schema in filtered.list_tools() {
        if at_depth_limit && COLLAB_TOOLS.contains(&schema.name.as_str()) {
            continue;
        }
        if let Some(tool) = parent.get(&schema.name) {
            registry.register_arc(tool);
        }
    }

    registry
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::mpsc;
    use tokio::time::Duration;

    use crate::llm::{
        CompletionRequest, CompletionResponse, FinishReason, LlmClient, MockLlmClient, MockStep,
        StreamResult, TokenUsage,
    };

    use super::super::tracker::SubagentTracker;
    use super::*;
    use restflow_traits::subagent::{
        SpawnPriority, SubagentDefLookup, SubagentDefSummary, SubagentStatus,
    };

    #[test]
    fn build_subagent_agent_config_sets_execution_context() {
        let config = build_subagent_agent_config(
            "Sub-task".to_string(),
            "System prompt".to_string(),
            3,
            None,
        );

        assert_eq!(
            config.context.get("execution_role"),
            Some(&serde_json::Value::String("subagent".to_string()))
        );
        assert_eq!(config.context["execution_context"]["role"], "subagent");
    }

    #[test]
    fn build_subagent_agent_config_sets_parent_execution_id_when_provided() {
        let config = build_subagent_agent_config(
            "Sub-task".to_string(),
            "System prompt".to_string(),
            3,
            Some("exec-parent-1"),
        );

        assert_eq!(
            config.context["execution_context"]["parent_execution_id"],
            "exec-parent-1"
        );
    }

    struct MockDefLookup {
        defs: HashMap<String, SubagentDefSnapshot>,
    }

    impl MockDefLookup {
        fn with_agent(id: &str) -> Self {
            let mut defs = HashMap::new();
            defs.insert(
                id.to_string(),
                SubagentDefSnapshot {
                    name: id.to_string(),
                    system_prompt: "You are a test agent.".to_string(),
                    allowed_tools: vec![],
                    max_iterations: Some(1),
                    default_model: None,
                },
            );
            Self { defs }
        }

        fn empty() -> Self {
            Self {
                defs: HashMap::new(),
            }
        }
    }

    impl SubagentDefLookup for MockDefLookup {
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
            self.defs.get(id).cloned()
        }

        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            vec![]
        }
    }

    #[test]
    fn resolve_subagent_definition_from_inline_config() {
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::empty());
        let tool_registry = Arc::new(ToolRegistry::new());
        let request = SpawnRequest {
            agent_id: None,
            inline: Some(InlineSubagentConfig {
                name: Some("tmp".to_string()),
                system_prompt: Some("Inline prompt".to_string()),
                allowed_tools: Some(vec!["http_request".to_string()]),
                max_iterations: Some(7),
            }),
            task: "test".to_string(),
            timeout_secs: None,
            priority: None,
            model: None,
            model_provider: None,
            parent_execution_id: None,
        };

        let snapshot = resolve_subagent_definition(&definitions, &tool_registry, &request)
            .expect("inline definition should resolve");
        assert_eq!(snapshot.name, "tmp");
        assert_eq!(snapshot.system_prompt, "Inline prompt");
        assert_eq!(snapshot.allowed_tools, vec!["http_request".to_string()]);
        assert_eq!(snapshot.max_iterations, Some(7));
    }

    #[tokio::test]
    async fn spawn_subagent_without_agent_id_uses_temporary_definition() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::empty());
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![MockStep::text("temporary done")],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: None,
                inline: None,
                task: "temporary task".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
            },
            None,
        )
        .expect("spawn should succeed without explicit agent");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("temporary subagent result should be available");
        assert!(result.success);
        assert_eq!(handle.agent_name, TEMPORARY_SUBAGENT_NAME);
    }

    #[derive(Clone)]
    struct ErrorFinishLlmClient;

    #[async_trait]
    impl LlmClient for ErrorFinishLlmClient {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-error-finish"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: Some(String::new()),
                tool_calls: vec![],
                finish_reason: FinishReason::Error,
                usage: Some(TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 0,
                    total_tokens: 1,
                    cost_usd: Some(0.0),
                }),
            })
        }

        fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
            panic!("complete_stream is not used in these tests");
        }

        fn supports_streaming(&self) -> bool {
            false
        }
    }

    #[test]
    fn spawn_request_serialization_round_trips() {
        let request = SpawnRequest {
            agent_id: Some("researcher".to_string()),
            inline: None,
            task: "Research topic X".to_string(),
            timeout_secs: Some(300),
            priority: Some(SpawnPriority::High),
            model: None,
            model_provider: None,
            parent_execution_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("researcher"));

        let parsed: SpawnRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id.as_deref(), Some("researcher"));
    }

    #[test]
    fn spawn_handle_serialization_round_trips() {
        let handle = SpawnHandle {
            id: "task-123".to_string(),
            agent_name: "Researcher".to_string(),
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("task-123"));
    }

    #[tokio::test]
    async fn spawn_over_max_parallel_does_not_execute() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![
                MockStep::text("result-1").with_delay(2000),
                MockStep::text("result-2"),
            ],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 1,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let result1 = spawn_subagent(
            tracker.clone(),
            definitions.clone(),
            llm_client.clone(),
            tool_registry.clone(),
            config.clone(),
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "first task".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
            },
            None,
        );
        assert!(result1.is_ok());

        let result2 = spawn_subagent(
            tracker.clone(),
            definitions.clone(),
            llm_client.clone(),
            tool_registry.clone(),
            config.clone(),
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "second task (should not execute)".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
            },
            None,
        );
        assert!(result2.is_err());

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(tracker.all().len(), 1);
    }

    #[tokio::test]
    async fn spawn_subagent_propagates_agent_failure_success_flag() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(ErrorFinishLlmClient);
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "force failure status".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
            },
            None,
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");

        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("LLM returned an error"));

        let state = tracker.get(&handle.id).expect("state should exist");
        assert_eq!(state.status, SubagentStatus::Failed);
    }

    #[tokio::test]
    async fn spawn_subagent_maps_max_iterations_to_failed_result() {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agent("tester"));
        let llm_client: Arc<dyn LlmClient> = Arc::new(MockLlmClient::from_steps(
            "mock",
            vec![MockStep::tool_call(
                "call-1",
                "missing_tool",
                serde_json::json!({"input":"x"}),
            )],
        ));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 2,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };

        let handle = spawn_subagent(
            tracker.clone(),
            definitions,
            llm_client,
            tool_registry,
            config,
            SpawnRequest {
                agent_id: Some("tester".to_string()),
                inline: None,
                task: "hit max iterations".to_string(),
                timeout_secs: Some(10),
                priority: None,
                model: None,
                model_provider: None,
                parent_execution_id: None,
            },
            None,
        )
        .expect("spawn should succeed");

        let result = tracker
            .wait(&handle.id)
            .await
            .expect("subagent result should be available");

        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Max iterations reached"));

        let state = tracker.get(&handle.id).expect("state should exist");
        assert_eq!(state.status, SubagentStatus::Failed);
    }

    #[test]
    fn build_registry_excludes_collab_tools_at_depth_limit() {
        let mut parent = ToolRegistry::new();

        struct DummyTool(&'static str);
        #[async_trait::async_trait]
        impl restflow_traits::Tool for DummyTool {
            fn name(&self) -> &str {
                self.0
            }

            fn description(&self) -> &str {
                ""
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn execute(
                &self,
                _input: serde_json::Value,
            ) -> std::result::Result<restflow_traits::ToolOutput, restflow_traits::ToolError>
            {
                unimplemented!()
            }
        }

        parent.register(DummyTool("http"));
        parent.register(DummyTool("bash"));
        parent.register(DummyTool("spawn_subagent"));
        parent.register(DummyTool("wait_subagents"));
        parent.register(DummyTool("list_subagents"));
        parent.register(DummyTool("cancel_agent"));
        parent.register(DummyTool("send_input"));

        let parent = Arc::new(parent);
        let all_tools: Vec<String> = vec![
            "http",
            "bash",
            "spawn_subagent",
            "wait_subagents",
            "list_subagents",
            "cancel_agent",
            "send_input",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let registry = build_registry_for_agent(&parent, &all_tools, 1, 1);
        let names: Vec<String> = registry
            .list_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert!(names.contains(&"http".to_string()));
        assert!(names.contains(&"bash".to_string()));
        assert!(!names.contains(&"spawn_subagent".to_string()));
        assert!(!names.contains(&"wait_subagents".to_string()));
        assert!(!names.contains(&"list_subagents".to_string()));
        assert!(!names.contains(&"cancel_agent".to_string()));
        assert!(!names.contains(&"send_input".to_string()));

        let registry = build_registry_for_agent(&parent, &all_tools, 0, 2);
        let names: Vec<String> = registry
            .list_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert!(names.contains(&"spawn_subagent".to_string()));
        assert!(names.contains(&"wait_subagents".to_string()));
    }

    #[test]
    fn subagent_config_disables_workspace_instruction_injection() {
        let config = build_subagent_agent_config(
            "task".to_string(),
            "You are subagent".to_string(),
            7,
            None,
        );
        assert_eq!(config.max_iterations, 7);
        assert_eq!(config.system_prompt.as_deref(), Some("You are subagent"));
        assert!(!config.prompt_flags.include_workspace_context);
        assert!(config.yolo_mode);
    }

    #[test]
    fn map_subagent_error_uses_default_message_on_missing_failure_error() {
        let mapped = map_subagent_error(false, None);
        assert_eq!(mapped.as_deref(), Some("Sub-agent execution failed"));
    }

    #[test]
    fn map_subagent_error_clears_error_on_success() {
        let mapped = map_subagent_error(true, Some("ignored".to_string()));
        assert!(mapped.is_none());
    }
}
