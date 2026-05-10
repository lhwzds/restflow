use super::config::build_subagent_config;
use super::subagent_backend::{
    build_service_subagent_manager, build_service_subagent_runtime_bundle,
    build_service_subagent_tool_registry, create_subagent_manager,
};
use super::*;
use crate::services::adapters::{AgentStoreAdapter, OpsProviderAdapter};
use ai::llm::{
    ClientKind, CompletionRequest, CompletionResponse, FinishReason, StreamChunk, StreamResult,
};
use async_trait::async_trait;
use futures::stream;
use redb::Database;
use serde_json::json;
use std::collections::HashSet;
use std::sync::RwLock;
use tempfile::tempdir;
use types::request::{
    AgentNode as ContractAgentNode, RunSpawnRequest as ContractRunSpawnRequest, WireModelRef,
};
use types::store::{AgentCreateRequest, AgentStore, AgentUpdateRequest};

struct DummyTool(&'static str);

#[async_trait]
impl types::Tool for DummyTool {
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
    ) -> std::result::Result<types::ToolOutput, types::ToolError> {
        unimplemented!()
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
    FileSessionStore,
    (),
    SecretStorage,
    ConfigStorage,
    AgentStorage,
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

    let session_storage = FileSessionStore::new(temp_dir.path().join("sessions")).unwrap();
    let test_master_key = std::array::from_fn(|index| (index as u8).wrapping_add(1));
    let secret_storage = SecretStorage::with_master_key(db.clone(), test_master_key).unwrap();
    let config_storage = ConfigStorage::new(db.clone()).unwrap();
    let agent_storage = AgentStorage::new(db.clone()).unwrap();

    unsafe {
        std::env::remove_var("RESTFLOW_DIR");
    }
    (
        session_storage,
        (),
        secret_storage,
        config_storage,
        agent_storage,
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

    async fn complete(&self, _request: CompletionRequest) -> ai::error::Result<CompletionResponse> {
        Ok(CompletionResponse {
            content: Some(self.response.clone()),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: None,
            reasoning_content: None,
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
    ) -> ai::error::Result<Arc<dyn LlmClient>> {
        if model == self.model {
            Ok(self.client.clone())
        } else {
            Err(ai::error::AiError::Llm(format!(
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
        _session_storage,
        _unused_storage_slot,
        _secret_storage,
        config_storage,
        _agent_storage,
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
    assert!(!registry.has("manage_sessions"));
    assert!(!registry.has("save_artifact"));
}

#[test]
fn test_create_tool_registry_excludes_subagent_tools_by_default() {
    let (
        _session_storage,
        _unused_storage_slot,
        _secret_storage,
        config_storage,
        _agent_storage,
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
    let allowlist = vec!["manage_agents".to_string()];
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
        _session_storage,
        _unused_storage_slot,
        secret_storage,
        _config_storage,
        agent_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_agents".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(agent_storage, secret_storage, known_tools);
    let base_node = types::AgentNode {
        model_ref: Some(types::ModelRef::from_model(types::ModelId::ClaudeSonnet4_5)),
        prompt: Some("You are a testing assistant".to_string()),
        temperature: Some(0.3),
        codex_cli_reasoning_effort: None,
        codex_cli_execution_mode: None,
        api_key_config: Some(types::ApiKeyConfig::Direct("test-key".to_string())),
        tools: Some(vec!["manage_agents".to_string()]),
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
                tools: Some(vec!["manage_agents".to_string()]),
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
        _session_storage,
        _unused_storage_slot,
        secret_storage,
        _config_storage,
        agent_storage,
        _temp_dir,
    ) = setup_storage();

    let known_tools = Arc::new(RwLock::new(
        ["manage_agents".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let adapter = AgentStoreAdapter::new(agent_storage, secret_storage, known_tools);

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

#[tokio::test(flavor = "current_thread")]
async fn test_create_tool_registry_uses_minimal_core_tool_surface() {
    let (
        _session_storage,
        _unused_storage_slot,
        _secret_storage,
        config_storage,
        _agent_storage,
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

#[tokio::test]
async fn test_service_subagent_manager_supports_temporary_model_provider_only() {
    let (
        _session_storage,
        _execution_context,
        _secret_storage,
        config_storage,
        agent_storage,
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
        _session_storage,
        _execution_context,
        secret_storage,
        config_storage,
        agent_storage,
        _temp_dir,
    ) = setup_storage();

    let service_registry =
        create_tool_registry(config_storage.clone(), None, None).expect("service registry");

    let bundle = build_service_subagent_runtime_bundle(
        agent_storage,
        &service_registry,
        build_llm_factory(Some(&secret_storage)),
        Arc::new(config_storage),
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
