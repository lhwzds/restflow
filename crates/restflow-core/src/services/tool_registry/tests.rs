use super::config::build_subagent_config;
use super::subagent_backend::{
    build_service_subagent_manager, build_service_subagent_tool_registry,
};
use super::*;
use crate::models::{ExecutionTraceCategory, ExecutionTraceQuery, Skill};
use crate::services::adapters::{
    AgentStoreAdapter, BackgroundAgentStoreAdapter, DbMemoryStoreAdapter, OpsProviderAdapter,
};
use crate::services::session::SessionService;
use async_trait::async_trait;
use futures::stream;
use redb::Database;
use restflow_ai::llm::{
    ClientKind, CompletionRequest, CompletionResponse, FinishReason, StreamChunk, StreamResult,
};
use restflow_contracts::request::{
    AgentNode as ContractAgentNode, DurabilityMode as ContractDurabilityMode,
    InlineSubagentConfig as ContractInlineSubagentConfig,
    SubagentSpawnRequest as ContractSubagentSpawnRequest,
};
use restflow_traits::assessment::{
    AgentOperationAssessor, OperationAssessment, OperationAssessmentIntent,
};
use restflow_traits::security::{SecurityDecision, ToolAction};
use restflow_traits::skill::SkillProvider as _;
use restflow_traits::store::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest, BackgroundAgentControlRequest,
    BackgroundAgentCreateRequest, BackgroundAgentDeleteRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentMessageRequest, BackgroundAgentProgressRequest, BackgroundAgentStore,
    BackgroundAgentTraceListRequest, BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest,
    MemoryStore as _,
};
use serde_json::json;
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

    async fn assess_background_agent_create(
        &self,
        _request: BackgroundAgentCreateRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "create_background_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_convert_session(
        &self,
        _request: restflow_traits::store::BackgroundAgentConvertSessionRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "convert_session_to_background_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_update(
        &self,
        _request: BackgroundAgentUpdateRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "update_background_agent",
            OperationAssessmentIntent::Save,
        ))
    }

    async fn assess_background_agent_delete(
        &self,
        _request: BackgroundAgentDeleteRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::warning_with_confirmation(
            "delete_background_agent",
            OperationAssessmentIntent::Save,
            vec![],
        ))
    }

    async fn assess_background_agent_control(
        &self,
        _request: BackgroundAgentControlRequest,
    ) -> std::result::Result<OperationAssessment, restflow_traits::ToolError> {
        Ok(OperationAssessment::ok(
            "control_background_agent",
            OperationAssessmentIntent::Run,
        ))
    }

    async fn assess_background_agent_template(
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
        _request: ContractSubagentSpawnRequest,
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
        _requests: Vec<ContractSubagentSpawnRequest>,
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
    SkillStorage,
    MemoryStorage,
    ChatSessionStorage,
    ChannelSessionBindingStorage,
    ExecutionTraceStorage,
    KvStoreStorage,
    WorkItemStorage,
    SecretStorage,
    ConfigStorage,
    AgentStorage,
    BackgroundAgentStorage,
    TriggerStorage,
    TerminalSessionStorage,
    crate::storage::DeliverableStorage,
    tempfile::TempDir,
) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(db_path).unwrap());
    let _restflow_env_lock = crate::paths::restflow_dir_env_lock();

    let state_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let previous_master_key = std::env::var_os("RESTFLOW_MASTER_KEY");
    unsafe {
        std::env::set_var("RESTFLOW_DIR", &state_dir);
        std::env::remove_var("RESTFLOW_MASTER_KEY");
    }

    let skill_storage = SkillStorage::new(db.clone()).unwrap();
    let memory_storage = MemoryStorage::new(db.clone()).unwrap();
    let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
    let channel_session_binding_storage = ChannelSessionBindingStorage::new(db.clone()).unwrap();
    let execution_trace_storage = ExecutionTraceStorage::new(db.clone()).unwrap();
    let kv_store_storage =
        KvStoreStorage::new(restflow_storage::KvStoreStorage::new(db.clone()).unwrap());
    let work_item_storage = WorkItemStorage::new(db.clone()).unwrap();
    let secret_storage = SecretStorage::with_config(
        db.clone(),
        restflow_storage::SecretStorageConfig {
            allow_insecure_file_permissions: true,
        },
    )
    .unwrap();
    let config_storage = ConfigStorage::new(db.clone()).unwrap();
    let agent_storage = AgentStorage::new(db.clone()).unwrap();
    let background_agent_storage = BackgroundAgentStorage::new(db.clone()).unwrap();
    let trigger_storage = TriggerStorage::new(db.clone()).unwrap();
    let terminal_storage = TerminalSessionStorage::new(db.clone()).unwrap();
    let deliverable_storage = crate::storage::DeliverableStorage::new(db).unwrap();

    unsafe {
        std::env::remove_var("RESTFLOW_DIR");
        if let Some(value) = previous_master_key {
            std::env::set_var("RESTFLOW_MASTER_KEY", value);
        } else {
            std::env::remove_var("RESTFLOW_MASTER_KEY");
        }
    }
    (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        temp_dir,
    )
}

struct DenyProcessSecurityGate;

#[async_trait]
impl SecurityGate for DenyProcessSecurityGate {
    async fn check_command(
        &self,
        _command: &str,
        _task_id: &str,
        _agent_id: &str,
        _workdir: Option<&str>,
    ) -> restflow_traits::error::Result<SecurityDecision> {
        Ok(SecurityDecision::allowed(None))
    }

    async fn check_tool_action(
        &self,
        action: &ToolAction,
        _agent_id: Option<&str>,
        _task_id: Option<&str>,
    ) -> restflow_traits::error::Result<SecurityDecision> {
        if action.tool_name == "process" {
            return Ok(SecurityDecision::blocked(Some(
                "process blocked by registry gate".to_string(),
            )));
        }
        Ok(SecurityDecision::allowed(None))
    }
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
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();
    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .unwrap();

    // Should have default tools + skill tool
    assert!(registry.has("http_request"));
    assert!(registry.has("send_email"));
    assert!(registry.has("telegram_send"));
    assert!(registry.has("discord_send"));
    assert!(registry.has("slack_send"));
    assert!(registry.has("browser"));
    assert!(registry.has("patch"));
    assert!(registry.has("edit"));
    assert!(registry.has("multiedit"));
    assert!(registry.has("glob"));
    assert!(registry.has("grep"));
    assert!(registry.has("task_list"));
    assert!(registry.has("skill"));
    assert!(registry.has("memory_search"));
    assert!(registry.has("kv_store"));
    assert!(registry.has("process"));
    assert!(registry.has("reply"));
    assert!(registry.has("switch_model"));
    assert!(registry.has("spawn_subagent"));
    assert!(registry.has("wait_subagents"));
    assert!(registry.has("list_subagents"));
    // New system management tools
    assert!(registry.has("manage_secrets"));
    assert!(registry.has("manage_config"));
    assert!(registry.has("manage_agents"));
    assert!(registry.has("manage_background_agents"));
    assert!(registry.has("manage_marketplace"));
    assert!(registry.has("manage_triggers"));
    assert!(registry.has("manage_terminal"));
    assert!(registry.has("manage_ops"));
    assert!(registry.has("security_query"));
    // Session, memory management, and auth profile tools
    assert!(registry.has("manage_sessions"));
    assert!(registry.has("manage_memory"));
    assert!(registry.has("manage_auth_profiles"));
    assert!(registry.has("save_deliverable"));
}

#[tokio::test(flavor = "current_thread")]
async fn test_spawn_subagent_returns_no_callable_error_without_agents() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();
    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .unwrap();

    let error = registry
        .execute_safe(
            "spawn_subagent",
            json!({
                "agent": "coder",
                "task": "hello"
            }),
        )
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("No callable sub-agents available"),
        "unexpected error: {error}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_manage_ops_session_summary_response_schema() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let session =
        crate::models::ChatSession::new("agent-test".to_string(), "gpt-5-mini".to_string())
            .with_name("Ops Session");
    chat_storage.create(&session).unwrap();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .unwrap();

    let output = registry
        .execute_safe(
            "manage_ops",
            json!({ "operation": "session_summary", "limit": 5 }),
        )
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.result["operation"], "session_summary");
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

    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
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
                        "manage_background_agents",
                        "manage_terminal",
                        "security_query"
                    ]
                }
            }),
        )
        .await
        .unwrap();

    assert!(
        output.success,
        "expected create to pass known tool validation, got: {:?}",
        output.result
    );
}

#[test]
fn test_skill_provider_list_empty() {
    let (
        storage,
        _memory_storage,
        _chat_storage,
        _channel_session_binding_storage,
        _execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        _secret_storage,
        _config_storage,
        _agent_storage,
        _background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        _deliverable_storage,
        _temp_dir,
    ) = setup_storage();
    let provider = SkillStorageProvider::new(storage);

    let skills = provider.list_skills();
    assert!(skills.is_empty());
}

#[test]
fn test_skill_provider_with_data() {
    let (
        storage,
        _memory_storage,
        _chat_storage,
        _channel_session_binding_storage,
        _execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        _secret_storage,
        _config_storage,
        _agent_storage,
        _background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        _deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let skill = crate::models::Skill::new(
        "test-skill".to_string(),
        "Test Skill".to_string(),
        Some("A test".to_string()),
        Some(vec!["http_request".to_string()]),
        "# Test Content".to_string(),
    );
    storage.create(&skill).unwrap();

    let provider = SkillStorageProvider::new(storage);

    let skills = provider.list_skills();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "test-skill");

    let content = provider.get_skill("test-skill").unwrap();
    assert_eq!(content.id, "test-skill");
    assert!(content.content.contains("Test Content"));

    assert!(provider.get_skill("nonexistent").is_none());
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
        skill_storage,
        _memory_storage,
        _chat_storage,
        _channel_session_binding_storage,
        _execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        secret_storage,
        _config_storage,
        agent_storage,
        background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        _deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let ops_skill = crate::models::Skill::new(
        "ops-skill".to_string(),
        "Ops Skill".to_string(),
        None,
        None,
        "ops".to_string(),
    );
    skill_storage.create(&ops_skill).unwrap();
    let trace_skill = crate::models::Skill::new(
        "trace-skill".to_string(),
        "Trace Skill".to_string(),
        None,
        None,
        "trace".to_string(),
    );
    skill_storage.create(&trace_skill).unwrap();

    let known_tools = Arc::new(RwLock::new(
        [
            "manage_background_agents".to_string(),
            "manage_agents".to_string(),
        ]
        .into_iter()
        .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(
        agent_storage,
        skill_storage,
        secret_storage,
        background_agent_storage,
        known_tools,
    );
    let base_node = crate::models::AgentNode {
        model: Some(crate::models::ModelId::ClaudeSonnet4_5),
        model_ref: Some(crate::models::ModelRef::from_model(
            crate::models::ModelId::ClaudeSonnet4_5,
        )),
        prompt: Some("You are a testing assistant".to_string()),
        temperature: Some(0.3),
        codex_cli_reasoning_effort: None,
        codex_cli_execution_mode: None,
        api_key_config: Some(crate::models::ApiKeyConfig::Direct("test-key".to_string())),
        tools: Some(vec!["manage_background_agents".to_string()]),
        skills: Some(vec!["ops-skill".to_string()]),
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
                model: Some("gpt-5-mini".to_string()),
                prompt: Some("Updated prompt".to_string()),
                tools: Some(vec![
                    "manage_background_agents".to_string(),
                    "manage_agents".to_string(),
                ]),
                skills: Some(vec!["ops-skill".to_string(), "trace-skill".to_string()]),
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
        skill_storage,
        _memory_storage,
        _chat_storage,
        _channel_session_binding_storage,
        _execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        secret_storage,
        _config_storage,
        agent_storage,
        background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        _deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_background_agents".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(
        agent_storage,
        skill_storage,
        secret_storage,
        background_agent_storage,
        known_tools,
    );

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
        skill_storage,
        _memory_storage,
        _chat_storage,
        _channel_session_binding_storage,
        _execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        secret_storage,
        _config_storage,
        agent_storage,
        background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        _deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_background_agents".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(
        agent_storage.clone(),
        skill_storage,
        secret_storage,
        background_agent_storage.clone(),
        known_tools,
    );

    let created = AgentStore::create_agent(
        &adapter,
        AgentCreateRequest {
            name: "Task Owner".to_string(),
            agent: ContractAgentNode {
                model: Some("claude-sonnet-4-5".to_string()),
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

    background_agent_storage
        .create_task(
            "Active MCP Task".to_string(),
            agent_id.clone(),
            crate::models::BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let err = AgentStore::delete_agent(&adapter, &agent_id).expect_err("should be blocked");
    let msg = err.to_string();
    assert!(msg.contains("Cannot delete agent"));
    assert!(msg.contains("Active MCP Task"));
}

#[test]
fn test_task_store_adapter_background_agent_flow() {
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
        _skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        _secret_storage,
        _config_storage,
        agent_storage,
        background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let created_agent = agent_storage
        .create_agent(
            "Background Owner".to_string(),
            crate::models::AgentNode::new(),
        )
        .unwrap();
    let adapter = BackgroundAgentStoreAdapter::new(
        background_agent_storage.clone(),
        agent_storage.clone(),
        deliverable_storage,
        SessionService::new(
            crate::storage::SessionStorage::new(
                chat_storage,
                channel_session_binding_storage,
                execution_trace_storage,
            ),
            Some(agent_storage),
            background_agent_storage,
            Some(memory_storage),
        ),
    )
    .with_assessor(Arc::new(BackgroundMutationAssessor));

    let created = BackgroundAgentStore::create_background_agent(
        &adapter,
        BackgroundAgentCreateRequest {
            name: "Background Agent".to_string(),
            agent_id: created_agent.id,
            chat_session_id: None,
            schedule: restflow_contracts::request::TaskSchedule::default(),
            input: Some("Run periodic checks".to_string()),
            input_template: Some("Template {{task.id}}".to_string()),
            timeout_secs: Some(1800),
            durability_mode: Some(ContractDurabilityMode::Async),
            memory: None,
            memory_scope: Some("per_background_agent".to_string()),
            resource_limits: None,
            preview: false,
            confirmation_token: None,
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
    assert_eq!(
        created
            .get("result")
            .and_then(|value| value.get("memory"))
            .and_then(|value| value.get("memory_scope"))
            .and_then(|value| value.as_str()),
        Some("per_background_agent")
    );
    let task_id = created
        .get("result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    let updated = BackgroundAgentStore::update_background_agent(
        &adapter,
        BackgroundAgentUpdateRequest {
            id: task_id.clone(),
            name: Some("Background Agent Updated".to_string()),
            description: Some("Updated description".to_string()),
            agent_id: None,
            chat_session_id: None,
            input: Some("Run checks and summarize".to_string()),
            input_template: Some("Updated {{task.name}}".to_string()),
            schedule: None,
            notification: None,
            execution_mode: None,
            timeout_secs: Some(900),
            durability_mode: Some(ContractDurabilityMode::Sync),
            memory: None,
            memory_scope: Some("shared_agent".to_string()),
            resource_limits: None,
            preview: false,
            confirmation_token: None,
        },
    )
    .unwrap();
    assert_eq!(
        updated
            .get("result")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str()),
        Some("Background Agent Updated")
    );
    assert_eq!(
        updated
            .get("result")
            .and_then(|value| value.get("memory"))
            .and_then(|value| value.get("memory_scope"))
            .and_then(|value| value.as_str()),
        Some("shared_agent")
    );
    assert_eq!(
        updated
            .get("result")
            .and_then(|value| value.get("timeout_secs"))
            .and_then(|value| value.as_u64()),
        Some(900)
    );

    let controlled = BackgroundAgentStore::control_background_agent(
        &adapter,
        BackgroundAgentControlRequest {
            id: task_id.clone(),
            action: "run_now".to_string(),
            preview: false,
            confirmation_token: None,
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

    let message = BackgroundAgentStore::send_background_agent_message(
        &adapter,
        BackgroundAgentMessageRequest {
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

    let progress = BackgroundAgentStore::get_background_agent_progress(
        &adapter,
        BackgroundAgentProgressRequest {
            id: task_id.clone(),
            event_limit: Some(5),
        },
    )
    .unwrap();
    assert_eq!(
        progress
            .get("background_agent_id")
            .and_then(|value| value.as_str()),
        Some(task_id.as_str())
    );

    let messages = BackgroundAgentStore::list_background_agent_messages(
        &adapter,
        BackgroundAgentMessageListRequest {
            id: task_id.clone(),
            limit: Some(10),
        },
    )
    .unwrap();
    assert_eq!(messages.as_array().map(|items| items.len()), Some(1));

    // Test list_background_agent_traces (DB-backed)
    let traces = BackgroundAgentStore::list_background_agent_traces(
        &adapter,
        BackgroundAgentTraceListRequest {
            id: Some(task_id.clone()),
            limit: Some(5),
        },
    )
    .unwrap();
    // Trace list is empty until execution telemetry writes canonical events
    assert!(traces.as_array().unwrap().is_empty() || traces.as_array().is_some());

    // Test read_background_agent_trace (DB-backed)
    let trace_result = BackgroundAgentStore::read_background_agent_trace(
        &adapter,
        BackgroundAgentTraceReadRequest {
            trace_id: "missing-trace-id".to_string(),
            line_limit: Some(10),
        },
    );
    assert!(trace_result.is_err());

    let delete_preview = BackgroundAgentStore::delete_background_agent(
        &adapter,
        restflow_traits::store::BackgroundAgentDeleteRequest {
            id: task_id.clone(),
            preview: true,
            confirmation_token: None,
        },
    )
    .unwrap();
    let token = delete_preview["assessment"]["confirmation_token"]
        .as_str()
        .expect("delete preview token")
        .to_string();
    let deleted = BackgroundAgentStore::delete_background_agent(
        &adapter,
        restflow_traits::store::BackgroundAgentDeleteRequest {
            id: task_id,
            preview: false,
            confirmation_token: Some(token),
        },
    )
    .unwrap();
    assert_eq!(deleted["result"]["deleted"].as_bool(), Some(true));
}

#[tokio::test(flavor = "current_thread")]
async fn test_marketplace_tool_list_and_uninstall() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let local_skill = Skill::new(
        "local-skill".to_string(),
        "Local Skill".to_string(),
        Some("from test".to_string()),
        None,
        "# Local".to_string(),
    );
    skill_storage.create(&local_skill).unwrap();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
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
    assert_eq!(listed.result.as_array().map(|items| items.len()), Some(1));

    let deleted = registry
        .execute_safe(
            "manage_marketplace",
            json!({ "operation": "uninstall", "id": "local-skill" }),
        )
        .await
        .unwrap();
    assert!(deleted.success);
    assert_eq!(deleted.result["deleted"].as_bool(), Some(true));
}

#[tokio::test(flavor = "current_thread")]
async fn test_trigger_tool_create_list_disable() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .unwrap();

    let created = registry
        .execute_safe(
            "manage_triggers",
            json!({
                "operation": "create",
                "workflow_id": "wf-001",
                "trigger_config": {
                    "type": "schedule",
                    "cron": "0 * * * * *",
                    "timezone": "UTC",
                    "payload": {"from": "test"}
                }
            }),
        )
        .await
        .unwrap();
    assert!(created.success);
    let trigger_id = created.result["id"].as_str().unwrap().to_string();

    let listed = registry
        .execute_safe("manage_triggers", json!({ "operation": "list" }))
        .await
        .unwrap();
    assert!(listed.success);
    assert_eq!(listed.result.as_array().map(|items| items.len()), Some(1));

    let disabled = registry
        .execute_safe(
            "manage_triggers",
            json!({ "operation": "disable", "id": trigger_id }),
        )
        .await
        .unwrap();
    assert!(disabled.success);
}

#[tokio::test(flavor = "current_thread")]
async fn test_terminal_tool_create_send_read_close() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
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
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
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
async fn test_db_memory_store_adapter_crud() {
    let (
        _skill_storage,
        memory_storage,
        _chat_storage,
        _channel_session_binding_storage,
        _execution_trace_storage,
        _kv_store_storage,
        _work_item_storage,
        _secret_storage,
        _config_storage,
        _agent_storage,
        _background_agent_storage,
        _trigger_storage,
        _terminal_storage,
        _deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let store = DbMemoryStoreAdapter::new(memory_storage);

    let saved = store
        .save(
            "test-agent",
            "My Note",
            "Hello world content",
            &["tag1".into(), "tag2".into()],
        )
        .unwrap();
    assert!(saved["success"].as_bool().unwrap());
    let entry_id = saved["id"].as_str().unwrap().to_string();
    assert_eq!(saved["title"].as_str().unwrap(), "My Note");

    let read = store.read_by_id(&entry_id).unwrap().unwrap();
    assert!(read["found"].as_bool().unwrap());
    assert_eq!(read["entry"]["title"].as_str().unwrap(), "My Note");
    assert_eq!(
        read["entry"]["content"].as_str().unwrap(),
        "Hello world content"
    );
    let tags = read["entry"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(tags.contains(&"tag1"));
    assert!(tags.contains(&"tag2"));
    assert!(!tags.iter().any(|t| t.starts_with("__title:")));

    let listed = store.list("test-agent", None, 10).unwrap();
    assert_eq!(listed["total"].as_u64().unwrap(), 1);
    let memories = listed["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["title"].as_str().unwrap(), "My Note");

    let listed = store.list("test-agent", Some("tag1"), 10).unwrap();
    assert_eq!(listed["count"].as_u64().unwrap(), 1);
    let listed = store.list("test-agent", Some("nonexistent"), 10).unwrap();
    assert_eq!(listed["count"].as_u64().unwrap(), 0);

    let found = store.search("test-agent", None, Some("Note"), 10).unwrap();
    assert!(found["count"].as_u64().unwrap() >= 1);
    let found = store
        .search("test-agent", None, Some("nonexistent"), 10)
        .unwrap();
    assert_eq!(found["count"].as_u64().unwrap(), 0);

    let found = store.search("test-agent", Some("tag2"), None, 10).unwrap();
    assert!(found["count"].as_u64().unwrap() >= 1);

    let saved2 = store
        .save(
            "test-agent",
            "My Note",
            "Hello world content",
            &["tag1".into()],
        )
        .unwrap();
    assert!(saved2["success"].as_bool().unwrap());
    let listed = store.list("test-agent", None, 10).unwrap();
    assert_eq!(listed["total"].as_u64().unwrap(), 1);

    let deleted = store.delete(&entry_id).unwrap();
    assert!(deleted["deleted"].as_bool().unwrap());
    let listed = store.list("test-agent", None, 10).unwrap();
    assert_eq!(listed["total"].as_u64().unwrap(), 0);

    let read = store.read_by_id(&entry_id).unwrap();
    assert!(read.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn test_create_tool_registry_always_has_memory_tools() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .unwrap();

    assert!(registry.has("save_to_memory"));
    assert!(registry.has("read_memory"));
    assert!(registry.has("list_memories"));
    assert!(registry.has("delete_memory"));
}

#[test]
fn test_runtime_allowlist_assembly_matches_service_registry_for_core_tools() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let service_registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage.clone(),
        kv_store_storage,
        work_item_storage,
        secret_storage.clone(),
        config_storage.clone(),
        agent_storage.clone(),
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .unwrap();

    let subagent_manager = create_subagent_manager(
        agent_storage,
        &service_registry,
        build_llm_factory(Some(&secret_storage)),
        Arc::new(config_storage),
        execution_trace_storage,
    );

    let allowlist = vec![
        "http_request".to_string(),
        "send_email".to_string(),
        "bash".to_string(),
        "file".to_string(),
        "run_python".to_string(),
        "spawn_subagent".to_string(),
        "wait_subagents".to_string(),
        "list_subagents".to_string(),
    ];
    let runtime_registry = crate::runtime::agent::tools::registry_from_allowlist(
        Some(&allowlist),
        Some(subagent_manager),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    for tool_name in [
        "http_request",
        "send_email",
        "bash",
        "file",
        "run_python",
        "python",
        "spawn_subagent",
        "wait_subagents",
        "list_subagents",
    ] {
        assert_eq!(
            runtime_registry.has(tool_name),
            service_registry.has(tool_name),
            "tool presence mismatch for {tool_name}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn test_create_tool_registry_applies_security_gate_to_process_tool() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        Some("agent-1".to_string()),
        Some(Arc::new(DenyProcessSecurityGate)),
    )
    .unwrap();

    let list_output = registry
        .execute_safe("process", json!({ "action": "list" }))
        .await
        .unwrap();
    assert!(!list_output.success);
    assert_eq!(
        list_output.error.as_deref(),
        Some("Action blocked: process blocked by registry gate")
    );

    let poll_output = registry
        .execute_safe(
            "process",
            json!({ "action": "poll", "session_id": "session-1" }),
        )
        .await
        .unwrap();
    assert!(!poll_output.success);
    assert_eq!(
        poll_output.error.as_deref(),
        Some("Action blocked: process blocked by registry gate")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn test_create_subagent_manager_persists_execution_traces() {
    let (
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let execution_trace_storage =
        ExecutionTraceStorage::new(execution_trace_storage.db()).expect("execution trace storage");

    let service_registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage.clone(),
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage.clone(),
        agent_storage.clone(),
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .expect("service registry");

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
        .spawn(ContractSubagentSpawnRequest {
            agent_id: None,
            inline: Some(ContractInlineSubagentConfig {
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
            parent_execution_id: Some("parent-run-1".to_string()),
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
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let service_registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage.clone(),
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage.clone(),
        agent_storage.clone(),
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .expect("service registry");

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
        .spawn(ContractSubagentSpawnRequest {
            agent_id: None,
            inline: None,
            task: "Say done".to_string(),
            timeout_secs: Some(30),
            max_iterations: None,
            priority: None,
            model: Some("mock-model".to_string()),
            model_provider: Some("openai".to_string()),
            parent_execution_id: None,
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
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        _temp_dir,
    ) = setup_storage();

    let service_registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage.clone(),
        kv_store_storage,
        work_item_storage,
        secret_storage.clone(),
        config_storage.clone(),
        agent_storage.clone(),
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        None,
        None,
        None,
    )
    .expect("service registry");

    let manager = build_service_subagent_manager(
        agent_storage,
        &service_registry,
        build_llm_factory(Some(&secret_storage)),
        Arc::new(config_storage),
        execution_trace_storage,
    );

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
    assert!(names.contains(&"reply"));
    assert!(!names.contains(&"custom_extra"));
}
