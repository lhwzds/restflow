use super::*;
use crate::auth::{AuthProvider, Credential, CredentialSource};
use crate::models::{AgentNode, SkillPreflightPolicyMode, SkillSource, TaskSchedule};
use crate::runtime::subagent::AgentDefinitionRegistry;
use crate::services::session::SessionService;
use crate::session_log::{FileSession, FileSessionStore};
use crate::test_support::RestflowTestEnv;
use ai::AiError;
use ai::agent::{SubagentConfig, SubagentTracker};
use std::future::Future;
#[cfg(unix)]
use std::path::PathBuf;
use std::pin::Pin;
use tokio::sync::mpsc;
use types::store::ReplySender;

fn create_test_storage() -> (Arc<Storage>, RestflowTestEnv) {
    let env = RestflowTestEnv::new();
    let db_path = env.db_path("test.db");
    let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
    (Arc::new(storage), env)
}

fn create_test_executor(storage: Arc<Storage>) -> AgentRuntimeExecutor {
    let auth_manager = Arc::new(AuthProfileManager::new(Arc::new(storage.secrets.clone())));
    let (completion_tx, completion_rx) = mpsc::channel(10);
    let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
    let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
    let subagent_config = SubagentConfig::default();
    AgentRuntimeExecutor::new(
        storage,
        Arc::new(ProcessRegistry::new()),
        auth_manager,
        subagent_tracker,
        subagent_definitions,
        subagent_config,
    )
}

#[cfg(unix)]
struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

#[cfg(unix)]
impl EnvVarGuard {
    fn set_path(key: &'static str, path: &std::path::Path) -> Self {
        let original = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, path);
        }
        Self { key, original }
    }
}

#[cfg(unix)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            unsafe {
                std::env::set_var(self.key, value);
            }
        } else {
            unsafe {
                std::env::remove_var(self.key);
            }
        }
    }
}

#[cfg(unix)]
#[derive(serde::Deserialize)]
struct TestSkrunSkill {
    id: String,
    name: String,
    version: String,
    kind: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    suggested_tools: Vec<String>,
    #[serde(default)]
    source_ref: Option<String>,
}

#[cfg(unix)]
fn install_skrun_skills(env: &RestflowTestEnv, response: &str) -> PathBuf {
    let root = env.root().join("skrun-skills");
    std::fs::create_dir_all(&root).unwrap();
    let records: Vec<TestSkrunSkill> = serde_json::from_str(response).unwrap();
    for record in records {
        let content = record
            .content
            .unwrap_or_else(|| format!("# {}", record.name));
        let mut artifact = match record.kind.as_str() {
            "markdown" => {
                skrun::SkillArtifact::markdown(record.id, record.name, record.version, content)
            }
            "rust_binary" => {
                let mut artifact =
                    skrun::SkillArtifact::rust_binary(record.id, record.name, record.version);
                artifact.content = Some(content);
                artifact
            }
            other => panic!("unsupported test skrun skill kind: {other}"),
        };
        artifact.suggested_tools = record.suggested_tools;
        artifact.source_ref = record.source_ref;
        skrun::save_artifact(root.join(&artifact.id), &artifact).unwrap();
    }
    root
}

#[test]
fn test_executor_creation() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    // Executor should be created successfully
    assert!(Arc::strong_count(&executor.storage) >= 1);
}

#[test]
fn load_chat_session_reads_file_session_without_materializing_to_redb() {
    let (storage, env) = create_test_storage();
    let file_store = FileSessionStore::new(env.root().join("sessions")).unwrap();
    let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    session.add_message(ChatMessage::user("run from jsonl"));
    file_store
        .write_session(&FileSession::from_chat_session(&session), false)
        .unwrap();
    let session_service = SessionService::new(
        storage.sessions.clone(),
        Some(storage.agents.clone()),
        storage.tasks.clone(),
    )
    .with_file_sessions(file_store);
    let executor = create_test_executor(storage.clone()).with_session_service(session_service);

    let loaded = executor.load_chat_session(&session.id).unwrap();

    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.messages.len(), 1);
    assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
}

#[test]
fn test_build_subagent_manager_attaches_shared_orchestrator() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let manager = executor.build_subagent_manager(
        Arc::new(CodexClient::new()),
        Arc::new(ToolRegistry::new()),
        Arc::new(DefaultLlmClientFactory::new(Default::default(), Vec::new())),
    );

    assert!(manager.orchestrator.is_some());
    assert!(manager.llm_client_factory.is_some());
}

#[test]
fn test_context_window_for_model() {
    assert_eq!(
        AgentRuntimeExecutor::context_window_for_model(ModelId::ClaudeSonnet4_5),
        200_000
    );
    assert_eq!(
        AgentRuntimeExecutor::context_window_for_model(ModelId::Gpt5),
        128_000
    );
    assert_eq!(
        AgentRuntimeExecutor::context_window_for_model(ModelId::DeepseekChat),
        64_000
    );
    assert_eq!(
        AgentRuntimeExecutor::context_window_for_model(ModelId::Gemini25Pro),
        1_000_000
    );
}

#[test]
fn test_chat_resource_limits_disable_wall_clock_when_unset() {
    let mapped = AgentRuntimeExecutor::chat_resource_limits(88, None);
    assert_eq!(mapped.max_tool_calls, 88);
    assert_eq!(mapped.max_wall_clock, Duration::ZERO);
    assert_eq!(mapped.max_cost_usd, None);
}

#[test]
fn test_chat_resource_limits_enable_wall_clock_when_set() {
    let mapped = AgentRuntimeExecutor::chat_resource_limits(99, Some(123));
    assert_eq!(mapped.max_tool_calls, 99);
    assert_eq!(mapped.max_wall_clock, Duration::from_secs(123));
    assert_eq!(mapped.max_cost_usd, None);
}

#[test]
fn test_apply_llm_timeout_sets_timeout_when_configured() {
    let config = ReActAgentConfig::new("goal".to_string());
    let config = AgentRuntimeExecutor::apply_llm_timeout(config, Some(600));
    assert_eq!(config.llm_timeout, Some(Duration::from_secs(600)));
}

#[test]
fn test_apply_llm_timeout_disables_timeout_when_unset() {
    let config =
        ReActAgentConfig::new("goal".to_string()).with_llm_timeout(Duration::from_secs(30));
    let config = AgentRuntimeExecutor::apply_llm_timeout(config, None);
    assert_eq!(config.llm_timeout, None);
}

#[test]
fn test_apply_execution_context_populates_context_keys() {
    let context = ExecutionContext::background("agent-1", "session-1", "task-1");
    let config = ReActAgentConfig::new("goal".to_string());
    let config = AgentRuntimeExecutor::apply_execution_context(config, &context);

    assert_eq!(
        config.context.get("execution_role"),
        Some(&serde_json::Value::String("task".to_string()))
    );
    assert_eq!(config.context["chat_session_id"], "session-1");
    assert_eq!(config.context["task_id"], "task-1");
    assert_eq!(config.context["execution_context"]["role"], "task");
}

#[test]
fn test_apply_execution_context_uses_parent_run_id_for_subagent_context() {
    let context = ExecutionContext::subagent("agent-2", "run-parent-1");
    let config = ReActAgentConfig::new("goal".to_string());
    let config = AgentRuntimeExecutor::apply_execution_context(config, &context);

    assert_eq!(config.context["parent_run_id"], "run-parent-1");
    assert_eq!(
        config.context["execution_context"]["parent_run_id"],
        "run-parent-1"
    );
}

#[test]
fn test_effective_max_tool_result_length_respects_small_requested_limit() {
    let value = AgentRuntimeExecutor::effective_max_tool_result_length(300, 128_000);
    assert_eq!(value, 300);
}

#[test]
fn test_effective_max_tool_result_length_clamps_large_requested_limit() {
    let value = AgentRuntimeExecutor::effective_max_tool_result_length(1_000_000, 128_000);
    assert_eq!(value, TOOL_RESULT_MAX_CHARS);
}

#[test]
fn test_effective_max_tool_result_length_for_small_context_window() {
    let value = AgentRuntimeExecutor::effective_max_tool_result_length(1_000_000, 2013);
    assert_eq!(value, 644);
}

struct NoopReplySender;

impl ReplySender for NoopReplySender {
    fn send(&self, _message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn test_filter_requested_tool_names_removes_reply_without_sender() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let requested = vec!["bash".to_string(), "reply".to_string(), "file".to_string()];

    let filtered = executor
        .filter_requested_tool_names(Some(&requested), false)
        .expect("filtered tool list");

    assert!(filtered.iter().any(|name| name == "bash"));
    assert!(filtered.iter().any(|name| name == "file"));
    assert!(!filtered.iter().any(|name| name == "reply"));
}

#[test]
fn test_filter_requested_tool_names_keeps_reply_with_sender() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage).with_reply_sender(Arc::new(NoopReplySender));
    let requested = vec!["reply".to_string(), "bash".to_string()];

    let filtered = executor
        .filter_requested_tool_names(Some(&requested), true)
        .expect("filtered tool list");

    assert!(filtered.iter().any(|name| name == "reply"));
    assert!(filtered.iter().any(|name| name == "bash"));
}

#[cfg(unix)]
#[test]
fn test_resolve_preflight_skills_includes_team_skrun_skill() {
    let (storage, temp_dir) = create_test_storage();
    let bin = install_skrun_skills(
        &temp_dir,
        r##"[{
          "id": "team",
          "name": "Team",
          "version": "0.1.0",
          "kind": "markdown",
          "content": "# Team\n\nUse spawn_subagent_batch.",
          "suggested_tools": ["spawn_subagent_batch"],
          "executable": false,
          "source_ref": "skrun:team@0.1.0"
        }]"##,
    );
    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
    let executor = create_test_executor(storage);

    let node = AgentNode {
        skills: Some(vec!["team".to_string()]),
        ..AgentNode::new()
    };
    let skills = executor.resolve_preflight_skills(&node, None).unwrap();

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "team");
    assert_eq!(skills[0].source, SkillSource::External);
    assert!(skills[0].read_only);
    assert_eq!(skills[0].source_ref.as_deref(), Some("skrun:team@0.1.0"));
    assert!(skills[0].content.contains("spawn_subagent_batch"));
}

#[cfg(unix)]
#[test]
fn test_resolve_effective_tool_names_activates_assigned_skrun_skill_tools() {
    let (storage, temp_dir) = create_test_storage();
    let bin = install_skrun_skills(
        &temp_dir,
        r##"[{
          "id": "manage-task",
          "name": "Manage Tasks",
          "version": "0.1.0",
          "kind": "markdown",
          "content": "# Manage Tasks",
          "suggested_tools": ["manage_tasks", "reply"],
          "executable": false
        }]"##,
    );
    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
    let executor = create_test_executor(storage);
    let node = AgentNode {
        skills: Some(vec!["manage-task".to_string()]),
        ..AgentNode::new()
    };

    let tools = executor
        .resolve_effective_tool_names(&node, None, None)
        .expect("assigned skrun skill should activate suggested tools");

    assert!(tools.iter().any(|tool| tool == "manage_tasks"));
    assert!(tools.iter().any(|tool| tool == "reply"));
}

#[cfg(unix)]
#[test]
fn test_resolve_effective_tool_names_activates_explicit_skill_mention() {
    let (storage, temp_dir) = create_test_storage();
    let bin = install_skrun_skills(
        &temp_dir,
        r##"[{
          "id": "manage-task",
          "name": "Manage Tasks",
          "version": "0.1.0",
          "kind": "markdown",
          "content": "# Manage Tasks",
          "suggested_tools": ["manage_tasks", "reply"],
          "executable": false
        }]"##,
    );
    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
    let executor = create_test_executor(storage);
    let node = AgentNode {
        skills: Some(vec!["manage-task".to_string()]),
        ..AgentNode::new()
    };

    let tools = executor
        .resolve_effective_tool_names(&node, None, Some("please use @manage-task"))
        .expect("explicit skill mention should activate suggested tools");

    assert!(tools.iter().any(|tool| tool == "load_skill"));
    assert!(tools.iter().any(|tool| tool == "manage_tasks"));
    assert!(tools.iter().any(|tool| tool == "reply"));
}

#[cfg(unix)]
#[test]
fn test_resolve_effective_tool_names_rejects_known_unassigned_skill_mention() {
    let (storage, temp_dir) = create_test_storage();
    let bin = install_skrun_skills(
        &temp_dir,
        r##"[{
          "id": "manage-task",
          "name": "Manage Tasks",
          "version": "0.1.0",
          "kind": "markdown",
          "content": "# Manage Tasks",
          "suggested_tools": ["manage_agents"],
          "executable": false
        }]"##,
    );
    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);
    let executor = create_test_executor(storage);
    let node = AgentNode::new();

    let tools = executor
        .resolve_effective_tool_names(&node, None, Some("please use @manage-task"))
        .expect("invalid skill mentions should not fail the turn");

    assert!(tools.iter().any(|tool| tool == "load_skill"));
    assert!(!tools.iter().any(|tool| tool == "manage_agents"));
}

#[tokio::test]
async fn test_resolve_primary_model_prefers_explicit_model() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let node = AgentNode::with_model(ModelId::ClaudeSonnet4_5);

    let resolved = executor.resolve_primary_model(&node).await.unwrap();
    assert_eq!(resolved, ModelId::ClaudeSonnet4_5);
}

#[tokio::test]
async fn test_resolve_primary_model_uses_openai_secret_when_model_missing() {
    let (storage, _temp_dir) = create_test_storage();
    storage
        .secrets
        .set_secret("OPENAI_API_KEY", "test-openai-key", None)
        .unwrap();
    let executor = create_test_executor(storage);
    let node = AgentNode::new();

    let resolved = executor.resolve_primary_model(&node).await.unwrap();
    assert_eq!(resolved, ModelId::Gpt5_4);
}

#[tokio::test]
async fn test_resolve_primary_model_uses_anthropic_opus_when_model_missing() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    executor
        .auth_manager
        .add_profile_from_credential(
            "anthropic-test",
            Credential::ApiKey {
                key: "test-anthropic-key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        )
        .await
        .unwrap();
    let node = AgentNode::new();

    let resolved = executor.resolve_primary_model(&node).await.unwrap();
    assert_eq!(resolved, ModelId::ClaudeOpus4_6);
}

#[test]
fn test_default_model_for_provider_uses_anthropic_opus() {
    assert_eq!(
        crate::models::provider_default_model(Provider::Anthropic),
        ModelId::ClaudeOpus4_6
    );
}

#[test]
fn test_default_model_for_provider_uses_minimax_m27() {
    assert_eq!(
        crate::models::provider_default_model(Provider::MiniMax),
        ModelId::MiniMaxM27
    );
}

#[tokio::test]
async fn test_executor_agent_not_found() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);

    let result = executor
        .execute("nonexistent-agent", None, None, None, None, None)
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("requires task_id"));
}

#[tokio::test]
async fn test_executor_no_api_key() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let result = executor
        .resolve_api_key_for_model(
            Provider::Anthropic,
            Some(&ApiKeyConfig::Secret("MISSING_TEST_SECRET".to_string())),
            Provider::Anthropic,
        )
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("MISSING_TEST_SECRET"),
        "Error should mention missing secret: {}",
        err_msg
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_execute_session_turn_enforces_skill_preflight_policy() {
    let (storage, temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let bin = install_skrun_skills(
        &temp_dir,
        r##"[{
          "id": "preflight-session-skill",
          "name": "Preflight Blocking Skill",
          "version": "0.1.0",
          "kind": "markdown",
          "content": "Use {{missing_input}} to proceed",
          "suggested_tools": ["missing_tool_for_test"],
          "executable": false
        }]"##,
    );
    let _skrun_bin = EnvVarGuard::set_path("SKRUN_SKILLS_DIR", &bin);

    let agent = AgentNode::with_model(ModelId::CodexCli)
        .with_skills(vec!["preflight-session-skill".to_string()])
        .with_skill_preflight_policy_mode(SkillPreflightPolicyMode::Enforce);
    let stored_agent = storage
        .agents
        .create_agent("session-preflight-agent".to_string(), agent)
        .unwrap();

    let mut session = ChatSession::new(
        stored_agent.id.clone(),
        ModelId::CodexCli.as_serialized_str().to_string(),
    );

    let result = executor
        .execute_session_turn_with_emitter_and_steer(
            &mut session,
            "run preflight check",
            16,
            SessionInputMode::EphemeralInput,
            None,
            SessionTurnRuntimeOptions::default(),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Preflight check failed"));
    assert!(err.contains("missing_tool"));
    assert!(err.contains("missing_input"));
}

#[tokio::test]
async fn test_resolve_api_key_requires_matching_zai_secret() {
    let (storage, _temp_dir) = create_test_storage();
    storage
        .secrets
        .set_secret("ZAI_CODING_PLAN_API_KEY", "zai-coding-plan-key", None)
        .unwrap();
    let executor = create_test_executor(storage);

    let result = executor
        .resolve_api_key_for_model(Provider::Zai, None, Provider::Zai)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_resolve_api_key_requires_matching_zai_coding_plan_secret() {
    let (storage, _temp_dir) = create_test_storage();
    storage
        .secrets
        .set_secret("ZAI_API_KEY", "zai-key", None)
        .unwrap();
    let executor = create_test_executor(storage);

    let result = executor
        .resolve_api_key_for_model(Provider::ZaiCodingPlan, None, Provider::ZaiCodingPlan)
        .await;

    assert!(result.is_err());
}

#[test]
fn test_validate_prerequisites_passes_with_completed_tasks() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let task_a = storage
        .tasks
        .create_task(
            "task-a".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Once { run_at: 0 },
        )
        .expect("first task should create");
    let task_b = storage
        .tasks
        .create_task(
            "task-b".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Once { run_at: 0 },
        )
        .expect("second task should create");
    storage
        .tasks
        .complete_task_execution(&task_a.id, Some("ok".to_string()), 1)
        .expect("first task should complete");
    storage
        .tasks
        .complete_task_execution(&task_b.id, Some("done".to_string()), 1)
        .expect("second task should complete");

    let prerequisites = vec![task_a.id, task_b.id];
    let result = executor.validate_prerequisites(&prerequisites);
    assert!(result.is_ok(), "validation should pass: {:?}", result.err());
}

#[test]
fn test_validate_prerequisites_rejects_incomplete_task() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let task = storage
        .tasks
        .create_task(
            "task-pending".to_string(),
            "agent-1".to_string(),
            TaskSchedule::default(),
        )
        .expect("task should create");

    let err = executor
        .validate_prerequisites(std::slice::from_ref(&task.id))
        .expect_err("validation should fail");
    assert!(err.to_string().contains(&format!("{} (active)", task.id)));
}

#[test]
fn test_validate_prerequisites_fails_when_missing() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let prerequisites = vec!["missing-task".to_string()];

    let err = executor
        .validate_prerequisites(&prerequisites)
        .expect_err("validation should fail");
    assert!(err.to_string().contains("missing-task (not found)"));
}

#[test]
fn test_is_credential_error_for_http_statuses() {
    let rate_limit = anyhow::Error::new(AiError::LlmHttp {
        provider: "anthropic".to_string(),
        status: 429,
        message: "rate limited".to_string(),
        retry_after_secs: Some(1),
    });
    assert!(!is_credential_error(&rate_limit));

    let unauthorized = anyhow::Error::new(AiError::LlmHttp {
        provider: "openai".to_string(),
        status: 401,
        message: "unauthorized".to_string(),
        retry_after_secs: None,
    });
    assert!(is_credential_error(&unauthorized));

    let server_error = anyhow::Error::new(AiError::LlmHttp {
        provider: "openai".to_string(),
        status: 500,
        message: "server error".to_string(),
        retry_after_secs: None,
    });
    assert!(!is_credential_error(&server_error));
}

#[test]
fn test_is_credential_error_for_llm_message_fallback() {
    let err = anyhow::Error::new(AiError::Llm("Rate limit exceeded".to_string()));
    assert!(!is_credential_error(&err));

    let err = anyhow::Error::new(AiError::Llm("context window exceeded".to_string()));
    assert!(!is_credential_error(&err));
}

// Note: test_build_tool_registry removed because build_tool_registry now requires
// an LlmClient for SubagentDeps. The core logic (registry_from_allowlist) is
// covered by integration tests in the daemon transport stack
