use super::*;
use crate::auth::{AuthProvider, Credential, CredentialSource};
use crate::models::{
    AgentNode, MemoryConfig, SharedEntry, Skill, SkillPreflightPolicyMode, Visibility,
};
use crate::runtime::subagent::AgentDefinitionRegistry;
use restflow_ai::agent::{SubagentConfig, SubagentTracker};
use restflow_traits::store::ReplySender;
use std::future::Future;
use std::pin::Pin;
use tempfile::tempdir;
use tokio::sync::mpsc;

fn create_test_storage() -> (Arc<Storage>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
    (Arc::new(storage), temp_dir)
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

fn create_trigger_skill(id: &str, trigger: &str, content: &str) -> Skill {
    let mut skill = Skill::new(
        id.to_string(),
        "Trigger Skill".to_string(),
        Some("triggered skill".to_string()),
        None,
        content.to_string(),
    );
    skill.triggers = vec![trigger.to_string()];
    skill
}

fn create_preflight_blocking_skill(id: &str) -> Skill {
    let mut skill = Skill::new(
        id.to_string(),
        "Preflight Blocking Skill".to_string(),
        Some("Skill with preflight blockers".to_string()),
        None,
        "Use {{missing_input}} to proceed".to_string(),
    );
    skill.suggested_tools = vec!["missing_tool_for_test".to_string()];
    skill
}

fn insert_shared_entry(storage: &Storage, key: &str, value: &str) {
    let now = Utc::now().timestamp_millis();
    let entry = SharedEntry {
        key: key.to_string(),
        value: value.to_string(),
        visibility: Visibility::Public,
        owner: None,
        content_type: Some("application/json".to_string()),
        type_hint: Some("deliverable".to_string()),
        tags: vec!["deliverable".to_string()],
        created_at: now,
        updated_at: now,
        last_modified_by: Some("test".to_string()),
    };
    storage.kv_store.set(&entry).unwrap();
}

#[test]
fn test_executor_creation() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    // Executor should be created successfully
    assert!(Arc::strong_count(&executor.storage) >= 1);
}

#[test]
fn test_build_subagent_deps_attaches_shared_orchestrator() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let deps = executor.build_subagent_deps(
        Arc::new(CodexClient::new()),
        Arc::new(ToolRegistry::new()),
        None,
    );

    assert!(deps.orchestrator.is_some());
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
fn test_spawn_request_from_plan_preserves_iteration_override() {
    let plan = ExecutionPlan {
        agent_id: Some("child".to_string()),
        input: Some("do work".to_string()),
        timeout_secs: Some(120),
        max_iterations: Some(77),
        ..ExecutionPlan::default()
    };

    let request = spawn_request_from_plan(&plan).expect("spawn request should build");

    assert_eq!(request.agent_id.as_deref(), Some("child"));
    assert_eq!(request.task, "do work");
    assert_eq!(request.timeout_secs, Some(120));
    assert_eq!(request.max_iterations, Some(77));
}

#[test]
fn test_to_agent_resource_limits_maps_cost_budget() {
    let limits = crate::models::ResourceLimits {
        max_tool_calls: 12,
        max_duration_secs: 34,
        max_output_bytes: 56,
        max_cost_usd: Some(7.5),
    };
    let mapped = AgentRuntimeExecutor::to_agent_resource_limits(&limits);
    assert_eq!(mapped.max_tool_calls, 12);
    assert_eq!(mapped.max_wall_clock, Duration::from_secs(34));
    assert_eq!(mapped.max_cost_usd, Some(7.5));
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
        Some(&serde_json::Value::String("background_agent".to_string()))
    );
    assert_eq!(config.context["chat_session_id"], "session-1");
    assert_eq!(config.context["background_task_id"], "task-1");
    assert_eq!(
        config.context["execution_context"]["role"],
        "background_agent"
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

#[test]
fn test_non_main_agent_prompt_flags_disable_workspace_injection() {
    let flags = AgentRuntimeExecutor::non_main_agent_prompt_flags();
    assert!(!flags.include_workspace_context);
    assert!(flags.include_base);
    assert!(flags.include_tools);
}

#[test]
fn test_truncate_ack_message_limits_length() {
    let short = AgentRuntimeExecutor::truncate_ack_message("  ok  ");
    assert_eq!(short, "ok");

    let long_input = "a".repeat(ACK_PHASE_MAX_CHARS + 20);
    let truncated = AgentRuntimeExecutor::truncate_ack_message(&long_input);
    assert_eq!(truncated.chars().count(), ACK_PHASE_MAX_CHARS + 3);
    assert!(truncated.ends_with("..."));
}

#[test]
fn test_build_ack_system_prompt_appends_phase_directive() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage);
    let node = AgentNode {
        prompt: Some("Base prompt".to_string()),
        ..AgentNode::new()
    };

    let prompt = executor.build_ack_system_prompt(&node, None).unwrap();
    assert!(prompt.contains("Base prompt"));
    assert!(prompt.contains("Temporary Acknowledgement Phase"));
    assert!(prompt.contains("Reply with exactly one short assistant message"));
}

/// Skills are now registered as callable tools, not injected into the prompt.
/// Triggered skills are resolved but do not appear in the system prompt.
#[test]
fn test_build_background_system_prompt_does_not_inject_triggered_skill() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let skill = create_trigger_skill("triggered-skill", "code review", "Triggered Content");
    storage.skills.create(&skill).unwrap();

    let node = AgentNode {
        prompt: Some("Base Prompt".to_string()),
        skills: Some(vec!["triggered-skill".to_string()]),
        ..AgentNode::new()
    };
    let prompt = executor
        .build_background_system_prompt(&node, None, None, Some("please do code review"))
        .unwrap();

    assert!(prompt.contains("Base Prompt"));
    // Skills are now tools, not injected into prompt
    assert!(!prompt.contains("Triggered Content"));
}

#[test]
fn test_build_background_system_prompt_skips_non_matching_skill() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let skill = create_trigger_skill("triggered-skill", "deploy release", "Triggered Content");
    storage.skills.create(&skill).unwrap();

    let node = AgentNode {
        prompt: Some("Base Prompt".to_string()),
        ..AgentNode::new()
    };
    let prompt = executor
        .build_background_system_prompt(&node, None, None, Some("review this patch"))
        .unwrap();

    assert!(prompt.contains("Base Prompt"));
    assert!(!prompt.contains("Triggered Content"));
}

/// SECURITY TEST: Triggered skills NOT in agent's skill list must be ignored
/// to prevent capability scope expansion via crafted input
#[test]
fn test_build_background_system_prompt_ignores_unauthorized_triggered_skill() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());

    // Create a privileged skill with trigger
    let privileged_skill = create_trigger_skill("privileged-skill", "admin", "Privileged Content");
    storage.skills.create(&privileged_skill).unwrap();

    // Agent does NOT have the privileged skill in its skill list
    let node = AgentNode {
        prompt: Some("Base Prompt".to_string()),
        skills: Some(vec!["regular-skill".to_string()]),
        ..AgentNode::new()
    };

    // User input triggers the privileged skill
    let prompt = executor
        .build_background_system_prompt(&node, None, None, Some("please do admin"))
        .unwrap();

    assert!(prompt.contains("Base Prompt"));
    // SECURITY: Privileged skill content must NOT be included
    assert!(!prompt.contains("Privileged Content"));
}

/// Even authorized triggered skills are not injected into prompt (skills are now tools).
#[test]
fn test_build_background_system_prompt_does_not_inject_authorized_triggered_skill() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());

    let skill = create_trigger_skill("authorized-skill", "code review", "Authorized Content");
    storage.skills.create(&skill).unwrap();

    let node = AgentNode {
        prompt: Some("Base Prompt".to_string()),
        skills: Some(vec!["authorized-skill".to_string()]),
        ..AgentNode::new()
    };

    let prompt = executor
        .build_background_system_prompt(&node, None, None, Some("please do code review"))
        .unwrap();

    assert!(prompt.contains("Base Prompt"));
    // Skills are now tools, not injected into prompt
    assert!(!prompt.contains("Authorized Content"));
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
    assert_eq!(resolved, ModelId::Gpt5);
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
        .execute(
            "nonexistent-agent",
            None,
            None,
            &MemoryConfig::default(),
            None,
        )
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
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

#[tokio::test]
async fn test_execute_session_turn_enforces_skill_preflight_policy() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let skill = create_preflight_blocking_skill("preflight-session-skill");
    storage.skills.create(&skill).unwrap();

    let agent = AgentNode::with_model(ModelId::CodexCli)
        .with_skills(vec![skill.id.clone()])
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
async fn test_execute_from_state_enforces_skill_preflight_policy() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    let skill = create_preflight_blocking_skill("preflight-resume-skill");
    storage.skills.create(&skill).unwrap();

    let agent = AgentNode::with_model(ModelId::CodexCli)
        .with_skills(vec![skill.id.clone()])
        .with_skill_preflight_policy_mode(SkillPreflightPolicyMode::Enforce);
    let stored_agent = storage
        .agents
        .create_agent("resume-preflight-agent".to_string(), agent)
        .unwrap();

    let state = restflow_ai::agent::AgentState::new("resume-state-test".to_string(), 8);
    let result = executor
        .execute_from_state(
            &stored_agent.id,
            None,
            state,
            &MemoryConfig::default(),
            None,
            None,
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
fn test_validate_prerequisites_passes_with_valid_deliverables() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    insert_shared_entry(
        &storage,
        "deliverable:task-a",
        r#"{"parts":[{"type":"text","content":"ok"}]}"#,
    );
    insert_shared_entry(
        &storage,
        "deliverable:task-b",
        r#"{"parts":[{"type":"text","content":"done"}]}"#,
    );

    let prerequisites = vec!["task-a".to_string(), "task-b".to_string()];
    let result = executor.validate_prerequisites(&prerequisites);
    assert!(result.is_ok(), "validation should pass: {:?}", result.err());
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
fn test_validate_prerequisites_fails_on_empty_parts() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    insert_shared_entry(&storage, "deliverable:task-empty", r#"{"parts":[]}"#);
    let prerequisites = vec!["task-empty".to_string()];

    let err = executor
        .validate_prerequisites(&prerequisites)
        .expect_err("validation should fail");
    assert!(err.to_string().contains("task-empty (empty deliverable)"));
}

#[test]
fn test_validate_prerequisites_fails_on_invalid_json() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());
    insert_shared_entry(&storage, "deliverable:task-invalid", "not-json");
    let prerequisites = vec!["task-invalid".to_string()];

    let err = executor
        .validate_prerequisites(&prerequisites)
        .expect_err("validation should fail");
    assert!(err.to_string().contains("task-invalid (invalid JSON)"));
}

#[test]
fn test_save_task_deliverable_persists_structured_payload() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = create_test_executor(storage.clone());

    executor
        .save_task_deliverable("task-save", "agent-1", "final answer")
        .expect("save deliverable should succeed");

    let entry = storage
        .kv_store
        .get_unchecked("deliverable:task-save")
        .expect("kv store read should succeed")
        .expect("deliverable entry should exist");
    assert_eq!(entry.type_hint.as_deref(), Some("deliverable"));
    assert_eq!(entry.owner.as_deref(), Some("agent-1"));

    let payload: serde_json::Value =
        serde_json::from_str(&entry.value).expect("payload should be valid json");
    assert_eq!(payload["agent_id"].as_str(), Some("agent-1"));
    let parts = payload["parts"]
        .as_array()
        .expect("parts should be an array");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0]["content"].as_str(), Some("final answer"));
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
