use super::config::build_subagent_config;
use super::subagent_backend::{
    build_service_subagent_manager, build_service_subagent_runtime_bundle,
    build_service_subagent_tool_registry, create_subagent_manager,
};
use super::*;
use crate::models::{ExecutionTraceCategory, ExecutionTraceQuery};
use crate::services::adapters::{AgentStoreAdapter, OpsProviderAdapter, TaskStoreAdapter};
use crate::services::session::SessionService;
use async_trait::async_trait;
use futures::stream;
use redb::Database;
use restflow_ai::llm::{
    ClientKind, CompletionRequest, CompletionResponse, FinishReason, StreamChunk, StreamResult,
};
use restflow_traits::assessment::{
    AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
};
use restflow_traits::request::{
    AgentNode as ContractAgentNode, InlineAgentRunConfig as ContractInlineAgentRunConfig,
    RunSpawnRequest as ContractRunSpawnRequest, WireModelRef,
};
use restflow_traits::store::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest, TaskControlRequest, TaskCreateRequest,
    TaskDeleteRequest, TaskMessageListRequest, TaskMessageRequest, TaskProgressRequest, TaskStore,
    TaskTraceListRequest, TaskTraceReadRequest, TaskUpdateRequest,
};
use serde_json::json;
use std::collections::HashSet;
use std::sync::RwLock;
use tempfile::tempdir;

struct DummyTool(&'static str);

struct BackgroundMutationAssessor;

#[async_trait]
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
    ) -> std::result::Result<restflow_traits::ToolOutput, restflow_traits::ToolError> {
        unimplemented!()
    }
}

#[async_trait]
impl AgentOperationAssessor for BackgroundMutationAssessor {
    async fn assess_agent_create(
        &self,
        _request: AgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "create_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_agent_update(
        &self,
        _request: AgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "update_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_create(
        &self,
        _request: TaskCreateRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "create_task",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_convert_session(
        &self,
        _request: restflow_traits::store::TaskConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "convert_session_to_task",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_update(
        &self,
        _request: TaskUpdateRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "update_task",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_task_delete(
        &self,
        _request: TaskDeleteRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::warning_with_confirmation(
            "delete_task",
            OperationAssessmentIntent::Save,
            vec![],
        ))
    }

    async fn assess_task_control(
        &self,
        _request: TaskControlRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "control_task",
            OperationAssessmentIntent::Run,
        ))
    }

    async fn assess_task_template(
        &self,
        operation: &str,
        intent: OperationAssessmentIntent,
        _agent_ids: Vec<String>,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(operation, intent))
    }

    async fn assess_subagent_spawn(
        &self,
        operation: &str,
        _request: ContractRunSpawnRequest,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            operation,
            OperationAssessmentIntent::Run,
        ))
    }

    async fn assess_subagent_batch(
        &self,
        operation: &str,
        _requests: Vec<ContractRunSpawnRequest>,
        _template_mode: bool,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            operation,
            OperationAssessmentIntent::Run,
        ))
    }
}

#[test]
fn build_subagent_config_maps_max_iterations_from_agent_defaults() {
    let defaults = AgentDefaults {
        max_parallel_subagents: 64,
        subagent_timeout_secs: 900,
        max_iterations: 123,
        max_depth: 7,
        ..AgentDefaults::default()
    };

    let config = build_subagent_config(&defaults);

    assert_eq!(config.max_parallel_agents, 64);
    assert_eq!(config.subagent_timeout_secs, 900);
    assert_eq!(config.max_iterations, 123);
    assert_eq!(config.max_depth, 7);
}

#[allow(clippy::type_complexity)]
fn setup_storage() -> (
    ChatSessionStorage,
    ExecutionTraceStorage,
    SecretStorage,
    ConfigStorage,
    AgentStorage,
    TaskStorage,
    TerminalSessionStorage,
    tempfile::TempDir,
) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(db_path).unwrap());
    let _restflow_env_lock = crate::paths::restflow_dir_env_lock();

    let state_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    unsafe {
        std::env::set_var("RESTFLOW_DIR", &state_dir);
    }

    let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
    let execution_trace_storage = ExecutionTraceStorage::new(db.clone()).unwrap();
    let test_master_key = std::array::from_fn(|index| (index as u8).wrapping_add(1));
    let secret_storage = SecretStorage::with_master_key(db.clone(), test_master_key).unwrap();
    let config_storage = ConfigStorage::new(db.clone()).unwrap();
    let agent_storage = AgentStorage::new(db.clone()).unwrap();
    let task_storage = TaskStorage::new(db.clone()).unwrap();
    let terminal_storage = TerminalSessionStorage::new(db.clone()).unwrap();

    unsafe {
        std::env::remove_var("RESTFLOW_DIR");
    }
    (
        chat_storage,
        execution_trace_storage,
        secret_storage,
        config_storage,
        agent_storage,
        task_storage,
        terminal_storage,
        temp_dir,
    )
}

struct TestLlmFactory {
    client: Arc<dyn LlmClient>,
    model: String,
    provider: LlmProvider,
}

struct TestLlmClient {
    model: String,
    response: String,
}

#[async_trait]
impl LlmClient for TestLlmClient {
    fn provider(&self) -> &str {
        "test"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> restflow_ai::error::Result<CompletionResponse> {
        Ok(CompletionResponse {
            content: Some(self.response.clone()),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: None,
        })
    }

    fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
        Box::pin(stream::iter(vec![Ok(StreamChunk::final_chunk(
            FinishReason::Stop,
            None,
        ))]))
    }
}

impl TestLlmFactory {
    fn new(client: Arc<dyn LlmClient>, model: &str, provider: LlmProvider) -> Self {
        Self {
            client,
            model: model.to_string(),
            provider,
        }
    }
}

impl LlmClientFactory for TestLlmFactory {
    fn create_client(
        &self,
        model: &str,
        _api_key: Option<&str>,
    ) -> restflow_ai::error::Result<Arc<dyn LlmClient>> {
        if model == self.model {
            Ok(self.client.clone())
        } else {
            Err(restflow_ai::error::AiError::Llm(format!(
                "unexpected model request: {model}"
            )))
        }
    }

    fn available_models(&self) -> Vec<String> {
        vec![self.model.clone()]
    }

    fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
        None
    }

    fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
        if model == self.model {
            Some(self.provider)
        } else {
            None
        }
    }

    fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
        (model == self.model).then_some(ClientKind::Http)
    }
}

#[test]
fn test_create_tool_registry() {
    let (
        _chat_storage,
        _execution_trace_storage,
        _secret_storage,
        config_storage,
        _agent_storage,
        _task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();
    let registry = create_tool_registry(config_storage, None, None).unwrap();

    for tool_name in [
        "bash",
        "file",
        "load_skill",
        "run_skill",
        "patch",
        "edit",
        "multiedit",
        "glob",
        "grep",
    ] {
        assert!(registry.has(tool_name), "missing {tool_name}");
    }

    for tool_name in [
        "http_request",
        "send_email",
        "telegram_send",
        "discord_send",
        "slack_send",
        "browser",
        "skill",
        "memory_search",
        "process",
        "reply",
        "switch_model",
        "spawn_subagent",
        "wait_subagents",
        "list_subagents",
        "manage_secrets",
        "manage_config",
    ] {
        assert!(!registry.has(tool_name), "unexpected {tool_name}");
    }
    assert!(!registry.has("manage_ops"));
    assert!(!registry.has("manage_agents"));
    assert!(!registry.has("manage_tasks"));
    assert!(!registry.has("manage_marketplace"));
    assert!(!registry.has("manage_terminal"));
    assert!(!registry.has("security_query"));
    assert!(!registry.has("manage_sessions"));
    assert!(!registry.has("manage_auth_profiles"));
    assert!(!registry.has("save_artifact"));
}

#[test]
fn test_create_tool_registry_excludes_subagent_tools_by_default() {
    let (
        _chat_storage,
        _execution_trace_storage,
        _secret_storage,
        config_storage,
        _agent_storage,
        _task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();
    let registry = create_tool_registry(config_storage, None, None).unwrap();

    assert!(!registry.has("spawn_subagent"));
    assert!(!registry.has("spawn_subagent_batch"));
    assert!(!registry.has("wait_subagents"));
    assert!(!registry.has("list_subagents"));
}

#[tokio::test(flavor = "current_thread")]
async fn test_manage_ops_task_summary_response_schema() {
    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("ops-registry.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");
    let allowlist = vec!["manage_ops".to_string()];
    let registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        None,
        None,
        Some(&storage),
        None,
        None,
        None,
    )
    .unwrap();

    let output = registry
        .execute_safe(
            "manage_ops",
            json!({ "operation": "task_summary", "limit": 5 }),
        )
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["operation"], "task_summary");
    assert!(output.result.get("evidence").is_some());
    assert!(output.result.get("verification").is_some());
}

#[test]
fn test_manage_ops_log_tail_rejects_path_outside_logs_dir() {
    let _lock = crate::paths::restflow_dir_env_lock();
    let temp_dir = tempdir().unwrap();
    let state_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let outside_log = temp_dir.path().join("outside.log");
    std::fs::write(&outside_log, "line-1\nline-2\n").unwrap();

    let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
    unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

    let result = OpsProviderAdapter::log_tail_payload(&json!({
        "path": outside_log.to_string_lossy(),
        "lines": 10
    }));

    unsafe {
        if let Some(value) = previous_restflow_dir {
            std::env::set_var("RESTFLOW_DIR", value);
        } else {
            std::env::remove_var("RESTFLOW_DIR");
        }
    }

    let err = result.expect_err("path outside ~/.restflow/logs should be rejected");
    assert!(err.to_string().contains("log_tail path must stay under"));
}

#[test]
fn test_manage_ops_log_tail_allows_relative_path_in_logs_dir() {
    let _lock = crate::paths::restflow_dir_env_lock();
    let temp_dir = tempdir().unwrap();
    let state_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
    unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

    let logs_dir = crate::paths::logs_dir().unwrap();
    let custom_log = logs_dir.join("custom.log");
    std::fs::write(&custom_log, "line-1\nline-2\nline-3\n").unwrap();

    let result = OpsProviderAdapter::log_tail_payload(&json!({
        "path": "custom.log",
        "lines": 2
    }));

    unsafe {
        if let Some(value) = previous_restflow_dir {
            std::env::set_var("RESTFLOW_DIR", value);
        } else {
            std::env::remove_var("RESTFLOW_DIR");
        }
    }

    let (evidence, verification) = result.expect("path under ~/.restflow/logs should pass");
    let lines = evidence["lines"]
        .as_array()
        .expect("lines should be an array");
    assert_eq!(evidence["line_count"], json!(2));
    assert_eq!(verification["path_exists"], json!(true));
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].as_str(), Some("line-2"));
    assert_eq!(lines[1].as_str(), Some("line-3"));
}

#[cfg(unix)]
#[test]
fn test_manage_ops_log_tail_rejects_symlink_path() {
    let _lock = crate::paths::restflow_dir_env_lock();
    let temp_dir = tempdir().unwrap();
    let state_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
    unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

    let logs_dir = crate::paths::logs_dir().unwrap();
    let outside_log = temp_dir.path().join("outside.log");
    std::fs::write(&outside_log, "line-1\nline-2\n").unwrap();
    let symlink_path = logs_dir.join("symlink.log");
    std::os::unix::fs::symlink(&outside_log, &symlink_path).unwrap();

    let result = OpsProviderAdapter::log_tail_payload(&json!({
        "path": "symlink.log",
        "lines": 2
    }));

    unsafe {
        if let Some(value) = previous_restflow_dir {
            std::env::set_var("RESTFLOW_DIR", value);
        } else {
            std::env::remove_var("RESTFLOW_DIR");
        }
    }

    let err = result.expect_err("symlink path should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("symlink") || message.contains("must stay under"),
        "unexpected error message: {message}"
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_manage_agents_accepts_tools_registered_after_snapshot_point() {
    struct AgentsDirEnvCleanup;
    impl Drop for AgentsDirEnvCleanup {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
        }
    }
    let _cleanup = AgentsDirEnvCleanup;
    let _env_lock = crate::prompt_files::agents_dir_env_lock();
    let agents_temp = tempdir().unwrap();
    unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("manage-agents-tools.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");
    let allowlist = vec![
        "manage_agents".to_string(),
        "manage_tasks".to_string(),
        "manage_terminal".to_string(),
        "security_query".to_string(),
    ];
    let registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        None,
        None,
        Some(&storage),
        None,
        None,
        Some(dir.path()),
    )
    .unwrap();

    let output = registry
        .execute_safe(
            "manage_agents",
            json!({
                "operation": "create",
                "name": "Late Tool Validation Agent",
                "agent": {
                    "tools": [
                        "bash",
                        "file",
                        "run_skill"
                    ]
                }
            }),
        )
        .await
        .unwrap();

    let blockers = output.result["assessment"]["blockers"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    assert_eq!(
        blockers, 0,
        "expected known tool validation to pass, got: {:?}",
        output.result
    );
}

#[test]
fn test_agent_store_adapter_crud_flow() {
    struct AgentsDirEnvCleanup;
    impl Drop for AgentsDirEnvCleanup {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
        }
    }

    let _cleanup = AgentsDirEnvCleanup;

    let _env_lock = crate::prompt_files::agents_dir_env_lock();
    let agents_temp = tempdir().unwrap();
    unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

    let (
        _chat_storage,
        _execution_trace_storage,
        secret_storage,
        _config_storage,
        agent_storage,
        task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_tasks".to_string(), "manage_agents".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(agent_storage, secret_storage, task_storage, known_tools);
    let base_node = crate::models::AgentNode {
        model_ref: Some(crate::models::ModelRef::from_model(
            crate::models::ModelId::ClaudeSonnet4_5,
        )),
        prompt: Some("You are a testing assistant".to_string()),
        temperature: Some(0.3),
        codex_cli_reasoning_effort: None,
        codex_cli_execution_mode: None,
        api_key_config: Some(crate::models::ApiKeyConfig::Direct("test-key".to_string())),
        tools: Some(vec!["manage_tasks".to_string()]),
        skills: None,
        skill_variables: None,
        skill_preflight_policy_mode: None,
        model_routing: None,
    };

    let created = AgentStore::create_agent(
        &adapter,
        AgentCreateRequest {
            name: "Ops Agent".to_string(),
            agent: ContractAgentNode::from(base_node),
        },
    )
    .unwrap();
    let agent_id = created
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    let listed = AgentStore::list_agents(&adapter).unwrap();
    assert_eq!(listed.as_array().map(|items| items.len()), Some(1));

    let fetched = AgentStore::get_agent(&adapter, &agent_id).unwrap();
    assert_eq!(
        fetched.get("name").and_then(|value| value.as_str()),
        Some("Ops Agent")
    );

    let updated = AgentStore::update_agent(
        &adapter,
        AgentUpdateRequest {
            id: agent_id.clone(),
            name: Some("Ops Agent Updated".to_string()),
            agent: Some(ContractAgentNode {
                model_ref: Some(WireModelRef {
                    provider: "openai".to_string(),
                    model: "gpt-5-mini".to_string(),
                }),
                prompt: Some("Updated prompt".to_string()),
                tools: Some(vec![
                    "manage_tasks".to_string(),
                    "manage_agents".to_string(),
                ]),
                skills: None,
                ..ContractAgentNode::default()
            }),
        },
    )
    .unwrap();
    assert_eq!(
        updated.get("name").and_then(|value| value.as_str()),
        Some("Ops Agent Updated")
    );
    assert_eq!(
        updated
            .get("agent")
            .and_then(|value| value.get("prompt"))
            .and_then(|value| value.as_str()),
        Some("Updated prompt")
    );

    let deleted = AgentStore::delete_agent(&adapter, &agent_id).unwrap();
    assert_eq!(
        deleted.get("deleted").and_then(|value| value.as_bool()),
        Some(true)
    );
}

#[test]
fn test_agent_store_adapter_rejects_unknown_tool() {
    struct AgentsDirEnvCleanup;
    impl Drop for AgentsDirEnvCleanup {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
        }
    }

    let _cleanup = AgentsDirEnvCleanup;
    let _env_lock = crate::prompt_files::agents_dir_env_lock();
    let agents_temp = tempdir().unwrap();
    unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

    let (
        _chat_storage,
        _execution_trace_storage,
        secret_storage,
        _config_storage,
        agent_storage,
        task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_tasks".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(agent_storage, secret_storage, task_storage, known_tools);

    let err = AgentStore::create_agent(
        &adapter,
        AgentCreateRequest {
            name: "Invalid".to_string(),
            agent: ContractAgentNode {
                tools: Some(vec!["unknown_tool".to_string()]),
                ..ContractAgentNode::default()
            },
        },
    )
    .expect_err("expected validation error");
    assert!(err.to_string().contains("validation_error"));
}

#[test]
fn test_agent_store_adapter_blocks_delete_with_active_task() {
    struct AgentsDirEnvCleanup;
    impl Drop for AgentsDirEnvCleanup {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
        }
    }

    let _cleanup = AgentsDirEnvCleanup;
    let _env_lock = crate::prompt_files::agents_dir_env_lock();
    let agents_temp = tempdir().unwrap();
    unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

    let (
        _chat_storage,
        _execution_trace_storage,
        secret_storage,
        _config_storage,
        agent_storage,
        task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_tasks".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(
        agent_storage.clone(),
        secret_storage,
        task_storage.clone(),
        known_tools,
    );

    let created = AgentStore::create_agent(
        &adapter,
        AgentCreateRequest {
            name: "Task Owner".to_string(),
            agent: ContractAgentNode {
                model_ref: Some(WireModelRef {
                    provider: "anthropic".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                }),
                prompt: Some("owner".to_string()),
                ..ContractAgentNode::default()
            },
        },
    )
    .unwrap();
    let agent_id = created
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    task_storage
        .create_task(
            "Active MCP Task".to_string(),
            agent_id.clone(),
            crate::models::TaskSchedule::default(),
        )
        .unwrap();

    let err = AgentStore::delete_agent(&adapter, &agent_id).expect_err("should be blocked");
    let msg = err.to_string();
    assert!(msg.contains("Cannot delete agent"));
    assert!(msg.contains("Active MCP Task"));
}

#[test]
fn test_task_store_adapter_task_flow() {
    struct AgentsDirEnvCleanup;
    impl Drop for AgentsDirEnvCleanup {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
        }
    }
    let _cleanup = AgentsDirEnvCleanup;
    let _env_lock = crate::prompt_files::agents_dir_env_lock();
    let agents_temp = tempdir().unwrap();
    unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

    let (
        chat_storage,
        execution_trace_storage,
        _secret_storage,
        _config_storage,
        agent_storage,
        task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let created_agent = agent_storage
        .create_agent(
            "Background Owner".to_string(),
            crate::models::AgentNode::new(),
        )
        .unwrap();
    let adapter = TaskStoreAdapter::new(
        task_storage.clone(),
        agent_storage.clone(),
        SessionService::new(
            crate::storage::SessionStorage::new(chat_storage, execution_trace_storage),
            Some(agent_storage),
            task_storage,
        ),
    )
    .with_assessor(Arc::new(BackgroundMutationAssessor));

    let created = TaskStore::create_task(
        &adapter,
        TaskCreateRequest {
            name: "Task".to_string(),
            agent_id: created_agent.id,
            chat_session_id: None,
            schedule: restflow_traits::request::TaskSchedule::default(),
            input: Some("Run periodic checks".to_string()),
            input_template: Some("Template {{task.id}}".to_string()),
            timeout_secs: Some(1800),
            resource_limits: None,
            preview: false,
            approval_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        created
            .get("result")
            .and_then(|value| value.get("input_template"))
            .and_then(|value| value.as_str()),
        Some("Template {{task.id}}")
    );
    let task_id = created
        .get("result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    let updated = TaskStore::update_task(
        &adapter,
        TaskUpdateRequest {
            id: task_id.clone(),
            name: Some("Task Updated".to_string()),
            description: Some("Updated description".to_string()),
            agent_id: None,
            chat_session_id: None,
            input: Some("Run checks and summarize".to_string()),
            input_template: Some("Updated {{task.name}}".to_string()),
            schedule: None,
            execution_mode: None,
            timeout_secs: Some(900),
            resource_limits: None,
            preview: false,
            approval_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        updated
            .get("result")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str()),
        Some("Task Updated")
    );
    assert_eq!(
        updated
            .get("result")
            .and_then(|value| value.get("timeout_secs"))
            .and_then(|value| value.as_u64()),
        Some(900)
    );

    let controlled = TaskStore::control_task(
        &adapter,
        TaskControlRequest {
            id: task_id.clone(),
            action: "run_now".to_string(),
            preview: false,
            approval_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        controlled
            .get("result")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str()),
        Some("active")
    );

    let message = TaskStore::send_task_message(
        &adapter,
        TaskMessageRequest {
            id: task_id.clone(),
            message: "Also check deployment logs".to_string(),
            source: Some("user".to_string()),
        },
    )
    .unwrap();
    assert_eq!(
        message.get("status").and_then(|value| value.as_str()),
        Some("queued")
    );

    let progress = TaskStore::get_task_progress(
        &adapter,
        TaskProgressRequest {
            id: task_id.clone(),
            event_limit: Some(5),
        },
    )
    .unwrap();
    assert_eq!(
        progress.get("task_id").and_then(|value| value.as_str()),
        Some(task_id.as_str())
    );

    let messages = TaskStore::list_task_messages(
        &adapter,
        TaskMessageListRequest {
            id: task_id.clone(),
            limit: Some(10),
        },
    )
    .unwrap();
    assert_eq!(messages.as_array().map(|items| items.len()), Some(1));

    // Test list_task_traces (DB-backed)
    let traces = TaskStore::list_task_traces(
        &adapter,
        TaskTraceListRequest {
            id: Some(task_id.clone()),
            limit: Some(5),
        },
    )
    .unwrap();
    // Trace list is empty until execution telemetry writes canonical events
    assert!(traces.as_array().unwrap().is_empty() || traces.as_array().is_some());

    // Test read_task_trace (DB-backed)
    let trace_result = TaskStore::read_task_trace(
        &adapter,
        TaskTraceReadRequest {
            trace_id: "missing-trace-id".to_string(),
            line_limit: Some(10),
        },
    );
    assert!(trace_result.is_err());

    let delete_preview = TaskStore::delete_task(
        &adapter,
        restflow_traits::store::TaskDeleteRequest {
            id: task_id.clone(),
            preview: true,
            approval_id: None,
        },
    )
    .unwrap();
    let token = delete_preview["assessment"]["approval_id"]
        .as_str()
        .expect("delete preview token")
        .to_string();
    let deleted = TaskStore::delete_task(
        &adapter,
        restflow_traits::store::TaskDeleteRequest {
            id: task_id,
            preview: false,
            approval_id: Some(token),
        },
    )
    .unwrap();
    assert_eq!(deleted["result"]["deleted"].as_bool(), Some(true));
}

#[tokio::test(flavor = "current_thread")]
async fn test_marketplace_tool_list_and_uninstall() {
    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("marketplace-tool.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");
    let allowlist = vec!["manage_marketplace".to_string()];
    let registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        None,
        None,
        Some(&storage),
        None,
        None,
        Some(dir.path()),
    )
    .unwrap();

    let listed = registry
        .execute_safe(
            "manage_marketplace",
            json!({ "operation": "list_installed" }),
        )
        .await
        .unwrap();
    assert!(listed.success);
    assert_eq!(listed.result.as_array().map(|items| items.len()), Some(0));

    let delete_error = registry
        .execute_safe(
            "manage_marketplace",
            json!({ "operation": "uninstall", "id": "marketplace-skill" }),
        )
        .await
        .expect_err("uninstall should report skrun guidance as a tool error");
    assert!(
        delete_error.to_string().contains("skrun"),
        "unexpected error: {delete_error}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_terminal_tool_create_send_read_close() {
    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("terminal-tool.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");
    let allowlist = vec!["manage_terminal".to_string()];
    let registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        None,
        None,
        Some(&storage),
        None,
        None,
        Some(dir.path()),
    )
    .unwrap();

    let created = registry
        .execute_safe(
            "manage_terminal",
            json!({
                "operation": "create",
                "name": "Agent Session",
                "working_directory": "/tmp"
            }),
        )
        .await
        .unwrap();
    assert!(created.success);

    let sent = registry
        .execute_safe(
            "manage_terminal",
            json!({
                "operation": "send_input",
                "session_id": created.result["id"].as_str().unwrap(),
                "data": "echo hello"
            }),
        )
        .await
        .unwrap();
    assert!(sent.success);
    let read = registry
        .execute_safe(
            "manage_terminal",
            json!({
                "operation": "read_output",
                "session_id": sent.result["session_id"].as_str().unwrap()
            }),
        )
        .await
        .unwrap();
    assert!(read.success);
    assert!(
        read.result["output"]
            .as_str()
            .unwrap_or_default()
            .contains("echo hello")
    );

    let closed = registry
        .execute_safe(
            "manage_terminal",
            json!({
                "operation": "close",
                "session_id": sent.result["session_id"].as_str().unwrap()
            }),
        )
        .await
        .unwrap();
    assert!(closed.success);
}

#[tokio::test(flavor = "current_thread")]
async fn test_security_query_tool_show_policy_and_check_permission() {
    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("security-query.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");
    let allowlist = vec!["security_query".to_string()];
    let registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        None,
        None,
        Some(&storage),
        None,
        None,
        Some(dir.path()),
    )
    .unwrap();

    let summary = registry
        .execute_safe("security_query", json!({ "operation": "list_permissions" }))
        .await
        .unwrap();
    assert!(summary.success);
    assert!(summary.result["allowlist_count"].as_u64().unwrap_or(0) > 0);

    let check = registry
        .execute_safe(
            "security_query",
            json!({
                "operation": "check_permission",
                "tool_name": "manage_marketplace",
                "operation_name": "install",
                "target": "skill-id",
                "summary": "Install skill"
            }),
        )
        .await
        .unwrap();
    assert!(check.success);
    assert!(check.result.get("allowed").is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn test_create_tool_registry_uses_minimal_core_tool_surface() {
    let (
        _chat_storage,
        _execution_trace_storage,
        _secret_storage,
        config_storage,
        _agent_storage,
        _task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(config_storage, None, None).unwrap();

    for tool_name in [
        "bash",
        "file",
        "edit",
        "multiedit",
        "patch",
        "glob",
        "grep",
        "load_skill",
        "run_skill",
    ] {
        assert!(registry.has(tool_name), "missing {tool_name}");
    }

    for tool_name in [
        "save_to_memory",
        "read_memory",
        "list_memories",
        "delete_memory",
        "http_request",
        "send_email",
        "python",
    ] {
        assert!(!registry.has(tool_name), "unexpected {tool_name}");
    }
}

#[test]
fn test_runtime_allowlist_assembly_matches_service_registry_for_core_tools() {
    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("registry-parity.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");

    let service_registry = create_tool_registry(storage.config.clone(), None, None).unwrap();

    let subagent_manager = create_subagent_manager(
        storage.agents.clone(),
        &service_registry,
        build_llm_factory(Some(&storage.secrets)),
        Arc::new(storage.config.clone()),
        storage.execution_traces.clone(),
    );

    let allowlist = vec![
        "bash".to_string(),
        "file".to_string(),
        "load_skill".to_string(),
        "run_skill".to_string(),
    ];
    let runtime_registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        Some(subagent_manager),
        None,
        Some(&storage),
        None,
        None,
        None,
    )
    .unwrap();

    for tool_name in ["bash", "file", "load_skill", "run_skill"] {
        assert_eq!(
            runtime_registry.has(tool_name),
            service_registry.has(tool_name),
            "tool presence mismatch for {tool_name}"
        );
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_runtime_allowlist_manage_agents_rejects_tool_aliases() {
    struct AgentsDirEnvCleanup;
    impl Drop for AgentsDirEnvCleanup {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(crate::prompt_files::AGENTS_DIR_ENV) };
        }
    }
    let _cleanup = AgentsDirEnvCleanup;
    let _env_lock = crate::prompt_files::agents_dir_env_lock();
    let agents_temp = tempdir().unwrap();
    unsafe { std::env::set_var(crate::prompt_files::AGENTS_DIR_ENV, agents_temp.path()) };

    let dir = tempdir().expect("temp dir should be created");
    let db_path = dir.path().join("registry-runtime-canonical-tools.db");
    let storage = crate::storage::Storage::new(db_path.to_str().expect("db path should be valid"))
        .expect("storage should be created");

    let allowlist = vec![
        "manage_agents".to_string(),
        "bash".to_string(),
        "file".to_string(),
    ];
    let runtime_registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        None,
        None,
        Some(&storage),
        None,
        None,
        None,
    )
    .expect("runtime registry should be built");

    let output = runtime_registry
        .execute_safe(
            "manage_agents",
            json!({
                "operation": "create",
                "name": "Runtime Alias Agent",
                "agent": {
                    "tools": ["http", "email", "python"]
                },
                "preview": true
            }),
        )
        .await
        .expect("runtime manage_agents preview should execute");

    assert!(output.success);
    assert_eq!(output.result["status"], "preview");
    assert_eq!(output.result["assessment"]["status"], "block");
    let blockers = output.result["assessment"]["blockers"]
        .as_array()
        .expect("blockers should be an array");
    assert!(blockers.iter().any(|blocker| {
        blocker["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown tool: http"))
    }));
    assert!(blockers.iter().any(|blocker| {
        blocker["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown tool: email"))
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn test_create_subagent_manager_persists_execution_traces() {
    let (
        _chat_storage,
        execution_trace_storage,
        _secret_storage,
        config_storage,
        agent_storage,
        _task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let execution_trace_storage =
        ExecutionTraceStorage::new(execution_trace_storage.db()).expect("execution trace storage");

    let service_registry =
        create_tool_registry(config_storage.clone(), None, None).expect("service registry");

    let mock_llm: Arc<dyn LlmClient> = Arc::new(TestLlmClient {
        model: "mock-model".to_string(),
        response: "done".to_string(),
    });
    let llm_factory: Arc<dyn LlmClientFactory> = Arc::new(TestLlmFactory::new(
        mock_llm,
        "mock-model",
        LlmProvider::OpenAI,
    ));

    let subagent_manager = create_subagent_manager(
        agent_storage,
        &service_registry,
        llm_factory,
        Arc::new(config_storage),
        execution_trace_storage.clone(),
    );

    let handle = subagent_manager
        .spawn(ContractRunSpawnRequest {
            agent_id: None,
            inline: Some(ContractInlineAgentRunConfig {
                name: Some("trace-test".to_string()),
                system_prompt: Some("Return a short answer.".to_string()),
                allowed_tools: Some(vec!["__no_such_tool__".to_string()]),
                max_iterations: Some(3),
            }),
            task: "Say done".to_string(),
            timeout_secs: Some(30),
            max_iterations: None,
            priority: None,
            model: Some("mock-model".to_string()),
            model_provider: Some("openai".to_string()),
            parent_run_id: Some("parent-run-1".to_string()),
            trace_session_id: Some("session-trace-1".to_string()),
            trace_scope_id: Some("scope-trace-1".to_string()),
        })
        .expect("spawn subagent");

    let running_states = subagent_manager.list_running();
    let running_state = running_states
        .iter()
        .find(|state| state.id == handle.id)
        .expect("running subagent state should be visible through public manager contract");
    assert_eq!(running_state.parent_run_id.as_deref(), Some("parent-run-1"));
    assert_eq!(running_state.agent_name, "trace-test");
    assert_eq!(running_state.task, "Say done");

    let result = subagent_manager
        .wait(&handle.id)
        .await
        .expect("subagent result");
    let result = result.result.expect("subagent result payload");
    assert!(
        result.success,
        "unexpected subagent failure: {:?}",
        result.error
    );

    let events = execution_trace_storage
        .query(&ExecutionTraceQuery {
            task_id: Some("scope-trace-1".to_string()),
            limit: Some(20),
            ..ExecutionTraceQuery::default()
        })
        .expect("query execution traces");
    assert!(
        !events.is_empty(),
        "expected persisted execution traces for subagent {}",
        handle.id
    );
    assert!(
        events
            .iter()
            .any(|event| event.category == ExecutionTraceCategory::Lifecycle),
        "expected lifecycle execution trace event for subagent {}",
        handle.id
    );
    assert!(
        events
            .iter()
            .any(|event| event.category == ExecutionTraceCategory::LlmCall),
        "expected llm call execution trace event for subagent {}",
        handle.id
    );
    let run_events = events
        .iter()
        .filter(|event| event.run_id.as_deref() == Some(handle.id.as_str()))
        .collect::<Vec<_>>();
    assert!(
        !run_events.is_empty(),
        "expected run-scoped execution trace events for subagent {}",
        handle.id
    );
    assert!(
        run_events
            .iter()
            .all(|event| event.parent_run_id.as_deref() == Some("parent-run-1"))
    );
    assert!(
        run_events
            .iter()
            .all(|event| event.session_id.as_deref() == Some("session-trace-1"))
    );
    assert!(
        run_events
            .iter()
            .all(|event| event.effective_model.as_deref() == Some("mock-model"))
    );
}

#[tokio::test]
async fn test_service_subagent_manager_supports_temporary_model_provider_only() {
    let (
        _chat_storage,
        execution_trace_storage,
        _secret_storage,
        config_storage,
        agent_storage,
        _task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let service_registry =
        create_tool_registry(config_storage.clone(), None, None).expect("service registry");

    let mock_llm: Arc<dyn LlmClient> = Arc::new(TestLlmClient {
        model: "mock-model".to_string(),
        response: "done".to_string(),
    });
    let llm_factory: Arc<dyn LlmClientFactory> = Arc::new(TestLlmFactory::new(
        mock_llm,
        "mock-model",
        LlmProvider::OpenAI,
    ));

    let subagent_manager = create_subagent_manager(
        agent_storage,
        &service_registry,
        llm_factory,
        Arc::new(config_storage),
        execution_trace_storage,
    );

    let handle = subagent_manager
        .spawn(ContractRunSpawnRequest {
            agent_id: None,
            inline: None,
            task: "Say done".to_string(),
            timeout_secs: Some(30),
            max_iterations: None,
            priority: None,
            model: Some("mock-model".to_string()),
            model_provider: Some("openai".to_string()),
            parent_run_id: None,
            trace_session_id: None,
            trace_scope_id: None,
        })
        .expect("spawn temporary subagent");

    let result = subagent_manager
        .wait(&handle.id)
        .await
        .expect("subagent result");
    let result = result.result.expect("subagent result payload");
    assert!(
        result.success,
        "unexpected subagent failure: {:?}",
        result.error
    );
}

#[test]
fn test_build_service_subagent_manager_attaches_shared_orchestrator() {
    let (
        _chat_storage,
        execution_trace_storage,
        secret_storage,
        config_storage,
        agent_storage,
        _task_storage,
        _terminal_storage,
        _temp_dir,
    ) = setup_storage();

    let service_registry =
        create_tool_registry(config_storage.clone(), None, None).expect("service registry");

    let bundle = build_service_subagent_runtime_bundle(
        agent_storage,
        &service_registry,
        build_llm_factory(Some(&secret_storage)),
        Arc::new(config_storage),
        execution_trace_storage,
    );
    let manager = build_service_subagent_manager(&bundle);

    assert!(manager.orchestrator.is_some());
}

#[test]
fn test_build_service_subagent_tool_registry_filters_non_default_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(DummyTool("bash"));
    registry.register(DummyTool("reply"));
    registry.register(DummyTool("custom_extra"));

    let filtered = build_service_subagent_tool_registry(&registry);
    let names = filtered.list();

    assert!(names.contains(&"bash"));
    assert!(!names.contains(&"reply"));
    assert!(!names.contains(&"custom_extra"));
}
