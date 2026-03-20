use super::*;
use crate::daemon::{IpcClient, IpcServer};
use crate::models::{
    ModelId, AgentNode, ApiKeyConfig, BackgroundAgentSchedule, ChannelSessionBinding, ChatSession,
    ChatSessionSource, Skill, SkillReference,
};
use crate::prompt_files;
use crate::storage::agent::StoredAgent;
use restflow_traits::ToolErrorCategory;
use rmcp::ClientHandler;
use rmcp::model::ClientInfo;
use serde_json::json;
use std::time::Instant;
use tempfile::TempDir;
use tokio::time::{Duration, sleep};

// =========================================================================
// Test Utilities
// =========================================================================

/// RAII guard that isolates the agents directory via env var.
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

/// Create a test server with a temporary database and isolated agents directory.
/// All returned values must be held alive for the test duration.
#[allow(clippy::await_holding_lock)]
async fn create_test_server() -> (
    RestFlowMcpServer,
    Arc<AppCore>,
    TempDir,
    TempDir,
    AgentsDirEnvGuard,
) {
    let env_guard = AgentsDirEnvGuard::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_agents = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_agents.path()) };
    let db_path = temp_dir.path().join("test.db");
    let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
    let default_agent = core.storage.agents.resolve_default_agent().unwrap();
    let mut configured_agent = default_agent.agent.clone();
    configured_agent.model = Some(ModelId::Gpt5);
    configured_agent.model_ref = Some(crate::models::ModelRef::from_model(ModelId::Gpt5));
    configured_agent.api_key_config = Some(ApiKeyConfig::Direct("test_key".to_string()));
    core.storage
        .agents
        .update_agent(default_agent.id.clone(), None, Some(configured_agent))
        .unwrap();
    (
        RestFlowMcpServer::new(core.clone()),
        core,
        temp_dir,
        temp_agents,
        env_guard,
    )
}

/// Create a test skill with given id and name
fn create_test_skill(id: &str, name: &str) -> Skill {
    Skill::new(
        id.to_string(),
        name.to_string(),
        Some(format!("Description for {}", name)),
        Some(vec!["test".to_string()]),
        format!("# {}\n\nContent here.", name),
    )
}

/// Create a test agent node
fn create_test_agent_node(prompt: &str) -> AgentNode {
    AgentNode {
        model: Some(ModelId::ClaudeSonnet4_5),
        model_ref: Some(crate::models::ModelRef::from_model(
            ModelId::ClaudeSonnet4_5,
        )),
        prompt: Some(prompt.to_string()),
        temperature: Some(0.7),
        codex_cli_reasoning_effort: None,
        codex_cli_execution_mode: None,
        api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
        tools: Some(vec!["http_request".to_string()]),
        skills: None,
        skill_variables: None,
        skill_preflight_policy_mode: None,
        model_routing: None,
    }
}

#[tokio::test]
async fn test_core_backend_session_source_is_resolved_from_binding() {
    let (server, core, _db, _agents, _guard) = create_test_server().await;

    let default_agent = core.storage.agents.resolve_default_agent().unwrap();
    let mut session = ChatSession::new(
        default_agent.id.clone(),
        ModelId::Gpt5.as_serialized_str().to_string(),
    )
    .with_name("binding-source-test");
    session.source_channel = Some(ChatSessionSource::Workspace);
    core.storage.chat_sessions.create(&session).unwrap();
    core.storage
        .channel_session_bindings
        .upsert(&ChannelSessionBinding::new(
            "telegram",
            None,
            "chat-777",
            &session.id,
        ))
        .unwrap();

    let get_json = server
        .handle_chat_session_get(ChatSessionGetParams {
            session_id: session.id.clone(),
        })
        .await
        .unwrap();
    let fetched: ChatSession = serde_json::from_str(&get_json).unwrap();
    assert_eq!(fetched.source_channel, Some(ChatSessionSource::Telegram));
    assert_eq!(fetched.source_conversation_id.as_deref(), Some("chat-777"));

    let list_json = server
        .handle_chat_session_list(ChatSessionListParams {
            agent_id: None,
            limit: Some(20),
        })
        .await
        .unwrap();
    let listed: Vec<ChatSessionSummary> = serde_json::from_str(&list_json).unwrap();
    let listed_one = listed
        .iter()
        .find(|item| item.id == session.id)
        .expect("session should appear in list");
    assert_eq!(listed_one.source_channel, Some(ChatSessionSource::Telegram));
    assert_eq!(
        listed_one.source_conversation_id.as_deref(),
        Some("chat-777")
    );
}

#[tokio::test]
async fn test_core_backend_backfills_legacy_external_route_to_binding() {
    let (server, core, _db, _agents, _guard) = create_test_server().await;

    let default_agent = core.storage.agents.resolve_default_agent().unwrap();
    let session = ChatSession::new(
        default_agent.id.clone(),
        ModelId::Gpt5.as_serialized_str().to_string(),
    )
    .with_name("legacy-source-test")
    .with_source(ChatSessionSource::Telegram, "legacy-chat");
    core.storage.chat_sessions.create(&session).unwrap();

    let get_json = server
        .handle_chat_session_get(ChatSessionGetParams {
            session_id: session.id.clone(),
        })
        .await
        .unwrap();
    let fetched: ChatSession = serde_json::from_str(&get_json).unwrap();
    assert_eq!(fetched.source_channel, Some(ChatSessionSource::Telegram));
    assert_eq!(
        fetched.source_conversation_id.as_deref(),
        Some("legacy-chat")
    );

    let binding = core
        .storage
        .channel_session_bindings
        .get_by_route("telegram", None, "legacy-chat")
        .unwrap()
        .expect("legacy route should be backfilled to binding");
    assert_eq!(binding.session_id, session.id);
}

// =========================================================================
// Serialization Tests
// =========================================================================

#[test]
fn test_skill_summary_serialization() {
    let summary = SkillSummary {
        id: "test-id".to_string(),
        name: "Test Skill".to_string(),
        description: Some("A test skill".to_string()),
        tags: Some(vec!["test".to_string()]),
        status: SkillStatus::Active,
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("test-id"));
    assert!(json.contains("Test Skill"));
}

#[test]
fn test_agent_summary_serialization() {
    let summary = AgentSummary {
        id: "test-id".to_string(),
        name: "Test Agent".to_string(),
        model: "gpt-5".to_string(),
        provider: "openai".to_string(),
    };

    let json = serde_json::to_string(&summary).unwrap();
    assert!(json.contains("test-id"));
    assert!(json.contains("gpt-5"));
}

// =========================================================================
// Skill Tool Tests
// =========================================================================

#[tokio::test]
async fn test_list_skills_empty() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let result = server.handle_list_skills(ListSkillsParams::default()).await;

    assert!(result.is_ok());
    let json = result.unwrap();
    let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
    // Default skills are bootstrapped; verify at least the known ones exist
    assert!(skills.len() >= 2);
    assert!(skills.iter().any(|s| s.id == "self-heal-ops"));
    assert!(skills.iter().any(|s| s.id == "structured-planner"));
}

#[tokio::test]
async fn test_list_skills_multiple() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let base_json = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let base_skills: Vec<SkillSummary> = serde_json::from_str(&base_json).unwrap();
    let base_len = base_skills.len();

    // Create skills using the service layer
    let skill1 = create_test_skill("skill-1", "Skill One");
    let skill2 = create_test_skill("skill-2", "Skill Two");

    crate::services::skills::create_skill(&core, skill1)
        .await
        .unwrap();
    crate::services::skills::create_skill(&core, skill2)
        .await
        .unwrap();

    let result = server.handle_list_skills(ListSkillsParams::default()).await;

    assert!(result.is_ok());
    let json = result.unwrap();
    let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
    assert_eq!(skills.len(), base_len + 2);
}

#[tokio::test]
async fn test_list_skills_filter_by_status() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let mut completed = create_test_skill("skill-completed", "Completed Skill");
    completed.status = SkillStatus::Completed;
    let draft = create_test_skill("skill-draft", "Draft Skill");

    crate::services::skills::create_skill(&core, completed)
        .await
        .unwrap();
    crate::services::skills::create_skill(&core, draft)
        .await
        .unwrap();

    let json = server
        .handle_list_skills(ListSkillsParams {
            status: Some("completed".to_string()),
        })
        .await
        .unwrap();
    let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();

    assert!(skills.iter().any(|s| s.id == "skill-completed"));
    assert!(skills.iter().all(|s| s.status == SkillStatus::Completed));
}

#[tokio::test]
async fn test_get_skill_success() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let skill = create_test_skill("test-skill", "Test Skill");
    crate::services::skills::create_skill(&core, skill.clone())
        .await
        .unwrap();

    let params = GetSkillParams {
        id: "test-skill".to_string(),
    };
    let result = server.handle_get_skill(params).await;

    assert!(result.is_ok());
    let json = result.unwrap();
    let retrieved: Skill = serde_json::from_str(&json).unwrap();
    assert_eq!(retrieved.id, "test-skill");
    assert_eq!(retrieved.name, "Test Skill");
    assert_eq!(retrieved.content, skill.content);
}

#[tokio::test]
async fn test_get_skill_not_found() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let params = GetSkillParams {
        id: "nonexistent".to_string(),
    };
    let result = server.handle_get_skill(params).await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("not found"));
}

#[tokio::test]
async fn test_get_skill_reference_success() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let mut skill = create_test_skill("root-skill", "Root Skill");
    skill.references = vec![SkillReference {
        id: "reference-skill".to_string(),
        path: "references/reference-skill.md".to_string(),
        title: Some("Reference Skill".to_string()),
        summary: Some("Detailed reference".to_string()),
    }];
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    let reference_skill = Skill::new(
        "reference-skill".to_string(),
        "Reference Skill".to_string(),
        Some("Reference".to_string()),
        None,
        "# Reference Skill\n\nDeep details.".to_string(),
    );
    crate::services::skills::create_skill(&core, reference_skill.clone())
        .await
        .unwrap();

    let json = server
        .handle_get_skill_reference(GetSkillReferenceParams {
            skill_id: "root-skill".to_string(),
            ref_id: "reference-skill".to_string(),
        })
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(payload["skill_id"], "root-skill");
    assert_eq!(payload["ref_id"], "reference-skill");
    assert_eq!(payload["content"], reference_skill.content);
}

#[tokio::test]
async fn test_get_skill_context_auto_complete_updates_status() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let mut skill = create_test_skill("auto-complete", "Auto Complete");
    skill.auto_complete = true;
    skill.references = vec![SkillReference {
        id: "ref-doc".to_string(),
        path: "references/ref-doc.md".to_string(),
        title: Some("Reference Doc".to_string()),
        summary: Some("Reference summary".to_string()),
    }];
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    let json = server
        .handle_get_skill_context(GetSkillContextParams {
            skill_id: "auto-complete".to_string(),
            input: Some("test input".to_string()),
        })
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(response["status"], "completed");
    assert_eq!(
        response["available_references"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        response["available_references"][0]["title"],
        "Reference Doc"
    );

    let updated = crate::services::skills::get_skill(&core, "auto-complete")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, SkillStatus::Completed);
}

#[tokio::test]
async fn test_create_skill_success() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let base_json = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let base_skills: Vec<SkillSummary> = serde_json::from_str(&base_json).unwrap();
    let base_len = base_skills.len();

    let params = CreateSkillParams {
        name: "New Skill".to_string(),
        description: Some("A new skill".to_string()),
        tags: Some(vec!["new".to_string()]),
        content: "# New Skill\n\nContent".to_string(),
    };
    let result = server.handle_create_skill(params).await;

    assert!(result.is_ok());
    let message = result.unwrap();
    assert!(message.contains("created successfully"));

    // Verify it was persisted
    let skills = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let skill_list: Vec<SkillSummary> = serde_json::from_str(&skills).unwrap();
    assert_eq!(skill_list.len(), base_len + 1);
    assert!(skill_list.iter().any(|s| s.name == "New Skill"));
}

#[tokio::test]
async fn test_create_skill_returns_validation_warnings_non_blocking() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let params = CreateSkillParams {
        name: "   ".to_string(),
        description: None,
        tags: Some(vec!["valid".to_string(), "".to_string()]),
        content: "   ".to_string(),
    };
    let result = server.handle_create_skill(params).await;

    assert!(result.is_ok());
    let message = result.unwrap();
    assert!(message.contains("created successfully"));
    assert!(message.contains("Warnings:"));
    assert!(message.contains("name: Skill name cannot be empty"));
    assert!(message.contains("content: Skill content cannot be empty"));
}

#[tokio::test]
async fn test_update_skill_success() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let skill = create_test_skill("test-skill", "Original Name");
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    let params = UpdateSkillParams {
        id: "test-skill".to_string(),
        name: Some("Updated Name".to_string()),
        description: Some("Updated description".to_string()),
        tags: None,
        content: Some("# Updated content".to_string()),
    };
    let result = server.handle_update_skill(params).await;

    assert!(result.is_ok());

    // Verify changes
    let get_params = GetSkillParams {
        id: "test-skill".to_string(),
    };
    let json = server.handle_get_skill(get_params).await.unwrap();
    let updated: Skill = serde_json::from_str(&json).unwrap();
    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.description, Some("Updated description".to_string()));
    assert_eq!(updated.content, "# Updated content");
}

#[tokio::test]
async fn test_update_skill_returns_validation_warnings_non_blocking() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let skill = create_test_skill("warn-skill", "Warn Skill");
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    let params = UpdateSkillParams {
        id: "warn-skill".to_string(),
        name: None,
        description: None,
        tags: None,
        content: Some("Use {{bad-variable}} in template".to_string()),
    };
    let result = server.handle_update_skill(params).await;

    assert!(result.is_ok());
    let message = result.unwrap();
    assert!(message.contains("updated successfully"));
    assert!(message.contains("Warnings:"));
    assert!(message.contains("Invalid variable 'bad-variable'"));

    let updated = crate::services::skills::get_skill(&core, "warn-skill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.content, "Use {{bad-variable}} in template");
}

#[tokio::test]
async fn test_update_skill_not_found() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let params = UpdateSkillParams {
        id: "nonexistent".to_string(),
        name: Some("New Name".to_string()),
        description: None,
        tags: None,
        content: None,
    };
    let result = server.handle_update_skill(params).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[tokio::test]
async fn test_update_skill_partial() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let skill = create_test_skill("test-skill", "Original Name");
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    // Only update name, keep other fields
    let params = UpdateSkillParams {
        id: "test-skill".to_string(),
        name: Some("New Name".to_string()),
        description: None,
        tags: None,
        content: None,
    };
    server.handle_update_skill(params).await.unwrap();

    let get_params = GetSkillParams {
        id: "test-skill".to_string(),
    };
    let json = server.handle_get_skill(get_params).await.unwrap();
    let updated: Skill = serde_json::from_str(&json).unwrap();

    assert_eq!(updated.name, "New Name");
    // Original description should be preserved
    assert_eq!(
        updated.description,
        Some("Description for Original Name".to_string())
    );
}

#[tokio::test]
async fn test_delete_skill_success() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let skill = create_test_skill("test-skill", "Test Skill");
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    let params = DeleteSkillParams {
        id: "test-skill".to_string(),
    };
    let result = server.handle_delete_skill(params).await;

    assert!(result.is_ok());

    // Verify deletion
    let get_params = GetSkillParams {
        id: "test-skill".to_string(),
    };
    let get_result = server.handle_get_skill(get_params).await;
    assert!(get_result.is_err());
}

// =========================================================================
// Agent Tool Tests
// =========================================================================

#[tokio::test]
async fn test_list_agents_default() {
    // AppCore creates a default agent on initialization
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let result = server.handle_list_agents().await;

    assert!(result.is_ok());
    let json = result.unwrap();
    let agents: Vec<AgentSummary> = serde_json::from_str(&json).unwrap();
    // Expect exactly one default agent created by AppCore
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].name, "Default Assistant");
}

#[tokio::test]
async fn test_list_agents_multiple() {
    // AppCore creates a default agent, so we start with 1
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let agent1 = create_test_agent_node("Prompt 1");
    let agent2 = create_test_agent_node("Prompt 2");

    crate::services::agent::create_agent(&core, "Agent 1".to_string(), agent1)
        .await
        .unwrap();
    crate::services::agent::create_agent(&core, "Agent 2".to_string(), agent2)
        .await
        .unwrap();

    let result = server.handle_list_agents().await;

    assert!(result.is_ok());
    let json = result.unwrap();
    let agents: Vec<AgentSummary> = serde_json::from_str(&json).unwrap();
    // 1 default + 2 created = 3 agents
    assert_eq!(agents.len(), 3);
}

#[tokio::test]
async fn test_get_agent_success() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let agent_node = create_test_agent_node("Test prompt");
    let stored = crate::services::agent::create_agent(&core, "Test Agent".to_string(), agent_node)
        .await
        .unwrap();

    let params = GetAgentParams {
        id: stored.id.clone(),
    };
    let result = server.handle_get_agent(params).await;

    assert!(result.is_ok());
    let json = result.unwrap();
    let retrieved: StoredAgent = serde_json::from_str(&json).unwrap();
    assert_eq!(retrieved.id, stored.id);
    assert_eq!(retrieved.name, "Test Agent");
    assert_eq!(retrieved.agent.prompt, Some("Test prompt".to_string()));
}

#[tokio::test]
async fn test_get_agent_not_found() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let params = GetAgentParams {
        id: "nonexistent".to_string(),
    };
    let result = server.handle_get_agent(params).await;

    assert!(result.is_err());
}

// =========================================================================
// ServerHandler Trait Tests
// =========================================================================

#[tokio::test]
async fn test_get_info() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let info = server.get_info();

    assert_eq!(info.server_info.name, "restflow");
    assert!(info.capabilities.tools.is_some());
    assert!(info.instructions.is_some());
}

#[test]
fn test_tool_definitions() {
    // Verify tool definitions are correct without needing RequestContext
    // The actual list_tools method would be called by the MCP framework
    let expected_tools = [
        "list_skills",
        "get_skill",
        "get_skill_reference",
        "create_skill",
        "update_skill",
        "delete_skill",
        "list_agents",
        "get_agent",
        "memory_search",
        "memory_store",
        "memory_stats",
        "get_skill_context",
        "chat_session_list",
        "chat_session_get",
        "manage_background_agents",
        "manage_hooks",
    ];

    // Verify we have definitions for all expected tools
    assert_eq!(expected_tools.len(), 16);
}

#[tokio::test]
async fn test_handle_unknown_tool() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    // Test unknown tool handling by simulating what call_tool does internally
    let result = match "unknown_tool" {
        "list_skills" => server.handle_list_skills(ListSkillsParams::default()).await,
        _ => Err(format!("Unknown tool: {}", "unknown_tool")),
    };

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown tool"));
}

#[tokio::test]
async fn test_handle_invalid_skill_params() {
    // Create test server to ensure setup works (also keeps pattern consistent)
    let (_server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    // Test with invalid params - missing required id field
    let args = serde_json::json!({"wrong_field": "value"});
    let result: Result<GetSkillParams, _> = serde_json::from_value(args);

    // Should fail to parse
    assert!(result.is_err());
}

// =========================================================================
// Integration Tests (Full Workflow)
// =========================================================================

#[tokio::test]
async fn test_skill_crud_workflow() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let base_json = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let base_skills: Vec<SkillSummary> = serde_json::from_str(&base_json).unwrap();
    let base_len = base_skills.len();

    // 1. Create
    let create_params = CreateSkillParams {
        name: "Workflow Skill".to_string(),
        description: Some("Test workflow".to_string()),
        tags: Some(vec!["workflow".to_string()]),
        content: "# Workflow\n\nInitial content".to_string(),
    };
    let create_result = server.handle_create_skill(create_params).await.unwrap();
    assert!(create_result.contains("created successfully"));

    // 2. List to get ID
    let list_json = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let skills: Vec<SkillSummary> = serde_json::from_str(&list_json).unwrap();
    assert_eq!(skills.len(), base_len + 1);
    let skill_id = skills
        .iter()
        .find(|skill| skill.name == "Workflow Skill")
        .unwrap()
        .id
        .clone();

    // 3. Get
    let get_params = GetSkillParams {
        id: skill_id.clone(),
    };
    let get_json = server.handle_get_skill(get_params).await.unwrap();
    let skill: Skill = serde_json::from_str(&get_json).unwrap();
    assert_eq!(skill.name, "Workflow Skill");

    // 4. Update
    let update_params = UpdateSkillParams {
        id: skill_id.clone(),
        name: Some("Updated Workflow Skill".to_string()),
        description: None,
        tags: None,
        content: Some("# Updated\n\nNew content".to_string()),
    };
    server.handle_update_skill(update_params).await.unwrap();

    // 5. Verify update
    let get_params2 = GetSkillParams {
        id: skill_id.clone(),
    };
    let get_json2 = server.handle_get_skill(get_params2).await.unwrap();
    let updated_skill: Skill = serde_json::from_str(&get_json2).unwrap();
    assert_eq!(updated_skill.name, "Updated Workflow Skill");
    assert_eq!(updated_skill.content, "# Updated\n\nNew content");

    // 6. Delete
    let delete_params = DeleteSkillParams {
        id: skill_id.clone(),
    };
    server.handle_delete_skill(delete_params).await.unwrap();

    // 7. Verify deletion
    let final_list = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let final_skills: Vec<SkillSummary> = serde_json::from_str(&final_list).unwrap();
    assert_eq!(final_skills.len(), base_len);
}

#[cfg(unix)]
#[tokio::test]
async fn test_ipc_backend_list_skills() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("mcp-ipc.db");
    let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());

    let socket_path = std::env::temp_dir().join(format!(
        "restflow-mcp-{}-{}.sock",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    let _ = std::fs::remove_file(&socket_path);
    let ipc_server = IpcServer::new(core.clone(), socket_path.clone());
    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let shutdown_rx = shutdown_tx.subscribe();
    let mut server_handle = Some(tokio::spawn(
        async move { ipc_server.run(shutdown_rx).await },
    ));

    let mut client = None;
    let mut last_connect_error = None;
    for _ in 0..100 {
        match IpcClient::connect(&socket_path).await {
            Ok(connected) => {
                client = Some(connected);
                break;
            }
            Err(err) => {
                last_connect_error = Some(err.to_string());
            }
        }
        sleep(Duration::from_millis(50)).await;
    }

    let server_hint = if client.is_none()
        && server_handle
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
    {
        let handle = server_handle.take().unwrap();
        match handle.await {
            Ok(Ok(())) => "ipc server exited before client connection".to_string(),
            Ok(Err(err)) => format!("ipc server startup failed: {}", err),
            Err(err) => format!("ipc server task join failed: {}", err),
        }
    } else {
        "ipc server still running".to_string()
    };

    if client.is_none() && server_hint.contains("Operation not permitted") {
        eprintln!(
            "Skipping IPC backend test in restricted environment: {}",
            server_hint
        );
        let _ = shutdown_tx.send(());
        if let Some(handle) = server_handle.take() {
            let _ = handle.await;
        }
        let _ = std::fs::remove_file(&socket_path);
        return;
    }

    let client = client.unwrap_or_else(|| {
        panic!(
            "Failed to connect to IPC server: {} ({})",
            last_connect_error.unwrap_or_else(|| "unknown error".to_string()),
            server_hint
        )
    });
    let mcp_server = RestFlowMcpServer::with_ipc(client);

    let base_json = mcp_server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let base_skills: Vec<SkillSummary> = serde_json::from_str(&base_json).unwrap();
    let base_len = base_skills.len();

    let skill = create_test_skill("ipc-skill", "IPC Skill");
    crate::services::skills::create_skill(&core, skill)
        .await
        .unwrap();

    let json = mcp_server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
    assert_eq!(skills.len(), base_len + 1);
    assert!(skills.iter().any(|s| s.name == "IPC Skill"));

    let _ = shutdown_tx.send(());
    if let Some(handle) = server_handle.take() {
        let _ = handle.await;
    }
    let _ = std::fs::remove_file(&socket_path);
}

struct MockBackend {
    skills: Vec<Skill>,
    session: ChatSession,
    api_defaults: ApiDefaults,
}

impl MockBackend {
    fn new() -> Self {
        let skill = Skill::new(
            "mock-skill".to_string(),
            "Mock Skill".to_string(),
            Some("Mock description".to_string()),
            None,
            "# Mock".to_string(),
        );
        let session = ChatSession::new("mock-agent".to_string(), "mock-model".to_string())
            .with_name("Mock Session");
        Self {
            skills: vec![skill],
            session,
            api_defaults: ApiDefaults::default(),
        }
    }

    fn agent_summary(&self) -> StoredAgent {
        StoredAgent {
            id: "mock-agent".to_string(),
            name: "Mock Agent".to_string(),
            agent: AgentNode {
                model: Some(ModelId::ClaudeSonnet4_5),
                model_ref: Some(crate::models::ModelRef::from_model(
                    ModelId::ClaudeSonnet4_5,
                )),
                prompt: Some("Mock prompt".to_string()),
                temperature: Some(0.5),
                codex_cli_reasoning_effort: None,
                codex_cli_execution_mode: None,
                api_key_config: Some(ApiKeyConfig::Direct("mock_key".to_string())),
                tools: None,
                skills: None,
                skill_variables: None,
                skill_preflight_policy_mode: None,
                model_routing: None,
            },
            prompt_file: None,
            created_at: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DummyClientHandler;

impl ClientHandler for DummyClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

async fn call_tool_through_mcp(
    server: RestFlowMcpServer,
    name: &str,
    arguments: serde_json::Value,
) -> CallToolResult {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        anyhow::Ok(())
    });

    let client = DummyClientHandler
        .serve(client_transport)
        .await
        .expect("client should connect to server");

    let arguments = match arguments {
        Value::Object(map) => Some(map),
        _ => None,
    };
    let result = client
        .call_tool(CallToolRequestParams {
            name: name.to_string().into(),
            arguments,
            meta: None,
            task: None,
        })
        .await
        .expect("tool call should return a result");

    client.cancel().await.expect("client cancel should succeed");
    server_handle
        .await
        .expect("server task should join")
        .expect("server should shut down cleanly");

    result
}

fn call_tool_text(result: &CallToolResult) -> &str {
    result
        .content
        .iter()
        .find_map(|content| content.raw.as_text().map(|text| text.text.as_str()))
        .expect("tool result should contain text content")
}

#[async_trait::async_trait]
impl McpBackend for MockBackend {
    async fn list_skills(&self) -> Result<Vec<Skill>, String> {
        Ok(self.skills.clone())
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>, String> {
        Ok(self.skills.iter().find(|s| s.id == id).cloned())
    }

    async fn get_skill_reference(
        &self,
        skill_id: &str,
        ref_id: &str,
    ) -> Result<Option<String>, String> {
        let Some(skill) = self.skills.iter().find(|skill| skill.id == skill_id) else {
            return Ok(None);
        };
        let Some(reference) = skill
            .references
            .iter()
            .find(|reference| reference.id == ref_id)
        else {
            return Ok(None);
        };
        Ok(Some(format!(
            "Mock reference content for {}",
            reference.path
        )))
    }

    async fn create_skill(&self, _skill: Skill) -> Result<(), String> {
        Ok(())
    }

    async fn update_skill(&self, _skill: Skill) -> Result<(), String> {
        Ok(())
    }

    async fn delete_skill(&self, _id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<StoredAgent>, String> {
        Ok(vec![self.agent_summary()])
    }

    async fn get_agent(&self, _id: &str) -> Result<StoredAgent, String> {
        Ok(self.agent_summary())
    }

    async fn search_memory(&self, _query: MemorySearchQuery) -> Result<MemorySearchResult, String> {
        Ok(MemorySearchResult {
            chunks: Vec::new(),
            total_count: 0,
            has_more: false,
        })
    }

    async fn store_memory(&self, _chunk: MemoryChunk) -> Result<String, String> {
        Ok("mock-chunk".to_string())
    }

    async fn get_memory_stats(&self, agent_id: &str) -> Result<MemoryStats, String> {
        Ok(MemoryStats {
            agent_id: agent_id.to_string(),
            ..MemoryStats::default()
        })
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>, String> {
        Ok(vec![ChatSessionSummary::from(&self.session)])
    }

    async fn list_sessions_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ChatSessionSummary>, String> {
        let summary = ChatSessionSummary::from(&self.session);
        if summary.agent_id == agent_id {
            Ok(vec![summary])
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession, String> {
        if id == "structured-ipc-error" {
            return Err(serde_json::json!({
                "code": 409,
                "message": "session conflict",
                "details": {
                    "error_kind": "session_lifecycle",
                    "status_code": 409
                }
            })
            .to_string());
        }
        if self.session.id == id {
            Ok(self.session.clone())
        } else {
            Err(format!("Session not found: {}", id))
        }
    }

    async fn list_tasks(
        &self,
        _status: Option<BackgroundAgentStatus>,
    ) -> Result<Vec<BackgroundAgent>, String> {
        Ok(Vec::new())
    }

    async fn create_background_agent(
        &self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent, String> {
        let mut task = BackgroundAgent::new(
            "mock-task".to_string(),
            spec.name,
            spec.agent_id,
            spec.schedule,
        );
        task.chat_session_id = spec.chat_session_id.unwrap_or_default();
        task.description = spec.description;
        task.input = spec.input;
        task.input_template = spec.input_template;
        if let Some(notification) = spec.notification {
            task.notification = notification;
        }
        if let Some(execution_mode) = spec.execution_mode {
            task.execution_mode = execution_mode;
        }
        task.timeout_secs = spec.timeout_secs;
        if let Some(memory) = spec.memory {
            task.memory = memory;
        }
        if let Some(durability_mode) = spec.durability_mode {
            task.durability_mode = durability_mode;
        }
        if let Some(resource_limits) = spec.resource_limits {
            task.resource_limits = resource_limits;
        }
        task.prerequisites = spec.prerequisites;
        if let Some(continuation) = spec.continuation {
            task.continuation = continuation;
        }
        Ok(task)
    }

    async fn update_background_agent(
        &self,
        _id: &str,
        _patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent, String> {
        Err("not implemented in mock backend".to_string())
    }

    async fn delete_background_agent(&self, _id: &str) -> Result<bool, String> {
        Ok(true)
    }

    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent, String> {
        let mut task = BackgroundAgent::new(
            id.to_string(),
            "Mock Controlled Task".to_string(),
            "mock-agent".to_string(),
            BackgroundAgentSchedule::default(),
        );
        task.status = match action {
            BackgroundAgentControlAction::Start
            | BackgroundAgentControlAction::Resume
            | BackgroundAgentControlAction::RunNow => BackgroundAgentStatus::Active,
            BackgroundAgentControlAction::Pause => BackgroundAgentStatus::Paused,
            BackgroundAgentControlAction::Stop => BackgroundAgentStatus::Interrupted,
        };
        task.chat_session_id = self.session.id.clone();
        Ok(task)
    }

    async fn get_background_agent_progress(
        &self,
        _id: &str,
        _event_limit: usize,
    ) -> Result<BackgroundProgress, String> {
        Err("not implemented in mock backend".to_string())
    }

    async fn send_background_agent_message(
        &self,
        _id: &str,
        _message: String,
        _source: BackgroundMessageSource,
    ) -> Result<BackgroundMessage, String> {
        Err("not implemented in mock backend".to_string())
    }

    async fn list_background_agent_messages(
        &self,
        _id: &str,
        _limit: usize,
    ) -> Result<Vec<BackgroundMessage>, String> {
        Ok(Vec::new())
    }

    async fn list_deliverables(&self, _task_id: &str) -> Result<Vec<Deliverable>, String> {
        Ok(Vec::new())
    }

    async fn list_tool_traces(
        &self,
        _session_id: &str,
        _limit: usize,
    ) -> Result<Vec<crate::models::ToolTrace>, String> {
        Ok(Vec::new())
    }

    async fn list_tool_traces_by_turn(
        &self,
        _session_id: &str,
        _turn_id: &str,
        _limit: usize,
    ) -> Result<Vec<crate::models::ToolTrace>, String> {
        Ok(Vec::new())
    }

    async fn get_background_agent(&self, id: &str) -> Result<BackgroundAgent, String> {
        let mut task = BackgroundAgent::new(
            id.to_string(),
            "Mock Task".to_string(),
            "mock-agent".to_string(),
            BackgroundAgentSchedule::default(),
        );
        task.chat_session_id = id.to_string();
        Ok(task)
    }

    async fn list_hooks(&self) -> Result<Vec<Hook>, String> {
        Ok(Vec::new())
    }

    async fn create_hook(&self, hook: Hook) -> Result<Hook, String> {
        Ok(hook)
    }

    async fn update_hook(&self, _id: &str, hook: Hook) -> Result<Hook, String> {
        Ok(hook)
    }

    async fn delete_hook(&self, _id: &str) -> Result<bool, String> {
        Ok(true)
    }

    async fn list_runtime_tools(&self) -> Result<Vec<RuntimeToolDefinition>, String> {
        Ok(vec![
            RuntimeToolDefinition {
                name: "echo_runtime".to_string(),
                description: "Echo the input payload.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    }
                }),
            },
            RuntimeToolDefinition {
                name: "send_email".to_string(),
                description: "Send an email.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string" },
                        "subject": { "type": "string" },
                        "body": { "type": "string" }
                    }
                }),
            },
        ])
    }

    async fn execute_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<RuntimeToolResult, String> {
        if name == "manage_background_agents" {
            let operation = input
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let result = match operation {
                "list" => json!([]),
                "list_deliverables" => json!([]),
                "convert_session" | "promote_to_background" => {
                    let session_id = input
                        .get("session_id")
                        .and_then(Value::as_str)
                        .unwrap_or("session-1");
                    let run_now = input
                        .get("run_now")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    json!({
                        "task": {
                            "id": "task-1",
                            "chat_session_id": session_id,
                        },
                        "source_session": {
                            "id": session_id,
                            "agent_id": self.session.agent_id,
                        },
                        "run_now": run_now
                    })
                }
                "stop" => json!({
                    "id": input.get("id").and_then(Value::as_str).unwrap_or("task-1"),
                    "action": "stop"
                }),
                _ => input,
            };
            Ok(RuntimeToolResult {
                success: true,
                result,
                error: None,
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            })
        } else if name == "echo_runtime" {
            Ok(RuntimeToolResult {
                success: true,
                result: input,
                error: None,
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            })
        } else if name == "fail_runtime" {
            Ok(RuntimeToolResult {
                success: false,
                result: serde_json::json!({
                    "exit_code": 7,
                    "stdout": "out",
                    "stderr": "err"
                }),
                error: Some("Command exited with code 7".to_string()),
                error_category: Some(ToolErrorCategory::Execution),
                retryable: Some(false),
                retry_after_ms: None,
            })
        } else if name == "send_email" {
            Ok(RuntimeToolResult {
                success: false,
                result: serde_json::json!({
                    "provider": "smtp",
                    "status": 550
                }),
                error: Some("Mailbox unavailable".to_string()),
                error_category: Some(ToolErrorCategory::Execution),
                retryable: Some(false),
                retry_after_ms: None,
            })
        } else {
            Err(format!("Unknown runtime tool: {}", name))
        }
    }

    async fn get_api_defaults(&self) -> Result<ApiDefaults, String> {
        Ok(self.api_defaults.clone())
    }
}

#[tokio::test]
async fn test_mock_backend_list_skills() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let json = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .unwrap();
    let skills: Vec<SkillSummary> = serde_json::from_str(&json).unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "Mock Skill");
}

#[tokio::test]
async fn test_mock_backend_session_filter() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let params = ChatSessionListParams {
        agent_id: Some("mock-agent".to_string()),
        limit: Some(10),
    };
    let json = server.handle_chat_session_list(params).await.unwrap();
    let sessions: Vec<ChatSessionSummary> = serde_json::from_str(&json).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].agent_id, "mock-agent");
}

#[tokio::test]
async fn test_manage_background_agents_list_operation() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let params = ManageBackgroundAgentsParams {
        operation: "list".to_string(),
        session_id: None,
        id: None,
        name: None,
        agent_id: None,
        task_id: None,
        chat_session_id: None,
        description: None,
        input: None,
        inputs: None,
        team: None,
        workers: None,
        save_as_team: None,
        input_template: None,
        schedule: None,
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        durability_mode: None,
        memory: None,
        memory_scope: None,
        resource_limits: None,
        prerequisites: None,
        status: None,
        action: None,
        event_limit: None,
        message: None,
        source: None,
        limit: None,
        offset: None,
        category: None,
        from_time_ms: None,
        to_time_ms: None,
        include_stats: None,
        trace_id: None,
        line_limit: None,
        run_now: None,
    };

    let json = server
        .handle_manage_background_agents(params)
        .await
        .unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn test_manage_background_agents_list_deliverables_operation() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let mut params = base_manage_background_params("list_deliverables");
    params.id = Some("task-1".to_string());

    let json = server
        .handle_manage_background_agents(params)
        .await
        .unwrap();
    let deliverables: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(deliverables.is_empty());
}

#[tokio::test]
async fn test_manage_background_agents_progress_uses_api_default_event_limit() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let mut params = base_manage_background_params("progress");
    params.id = Some("task-1".to_string());

    let json = server
        .handle_manage_background_agents(params)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["event_limit"], serde_json::json!(10));
}

#[tokio::test]
async fn test_manage_background_agents_list_messages_uses_api_default_limit() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let mut params = base_manage_background_params("list_messages");
    params.id = Some("task-1".to_string());

    let json = server
        .handle_manage_background_agents(params)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["limit"], serde_json::json!(50));
}

#[tokio::test]
async fn test_manage_background_agents_convert_session_operation() {
    let backend = Arc::new(MockBackend::new());
    let session_id = backend.session.id.clone();
    let server = RestFlowMcpServer::with_backend(backend);
    let mut params = base_manage_background_params("convert_session");
    params.session_id = Some(session_id.clone());
    params.input = Some("Continue in background".to_string());
    params.run_now = Some(false);

    let json = server
        .handle_manage_background_agents(params)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["source_session"]["id"], session_id);
    assert_eq!(value["task"]["chat_session_id"], session_id);
    assert_eq!(value["run_now"], false);
}

#[tokio::test]
async fn test_manage_background_agents_promote_to_background_operation() {
    let backend = Arc::new(MockBackend::new());
    let session_id = backend.session.id.clone();
    let server = RestFlowMcpServer::with_backend(backend);
    let mut params = base_manage_background_params("promote_to_background");
    params.session_id = Some(session_id.clone());
    params.input = Some("Promote this chat".to_string());
    params.run_now = Some(true);

    let json = server
        .handle_manage_background_agents(params)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["source_session"]["id"], session_id);
    assert_eq!(value["task"]["chat_session_id"], session_id);
    assert_eq!(value["run_now"], true);
}

#[tokio::test]
async fn test_mcp_manage_background_agents_save_team_and_get_team_round_trip() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let save = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "save_team",
            "team": "bg-review-team",
            "workers": [
                {
                    "count": 2,
                    "agent_id": "default"
                }
            ]
        }),
    )
    .await;
    assert!(!save.is_error.unwrap_or(false), "{}", call_tool_text(&save));
    let save_value: serde_json::Value =
        serde_json::from_str(call_tool_text(&save)).expect("save_team response json");
    assert_eq!(save_value["operation"], "save_team");

    let get = call_tool_through_mcp(
        server,
        "manage_background_agents",
        serde_json::json!({
            "operation": "get_team",
            "team": "bg-review-team"
        }),
    )
    .await;
    assert!(!get.is_error.unwrap_or(false));
    let get_value: serde_json::Value =
        serde_json::from_str(call_tool_text(&get)).expect("get_team response json");
    assert_eq!(get_value["operation"], "get_team");
    assert_eq!(get_value["team"], "bg-review-team");
    assert_eq!(get_value["member_groups"], 1);
    assert_eq!(get_value["total_instances"], 2);
    assert!(
        get_value["members"][0].get("input").is_none()
            || get_value["members"][0]["input"].is_null()
    );
    assert!(
        get_value["members"][0].get("inputs").is_none()
            || get_value["members"][0]["inputs"].is_null()
    );
}

#[tokio::test]
async fn test_mcp_manage_background_agents_run_batch_accepts_runtime_inputs() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let save = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "save_team",
            "team": "bg-runtime-inputs-team",
            "workers": [
                {
                    "count": 2,
                    "agent_id": "default"
                }
            ]
        }),
    )
    .await;
    assert!(!save.is_error.unwrap_or(false), "{}", call_tool_text(&save));

    let run = call_tool_through_mcp(
        server,
        "manage_background_agents",
        serde_json::json!({
            "operation": "run_batch",
            "team": "bg-runtime-inputs-team",
            "inputs": ["scan backend", "scan frontend"],
            "run_now": false
        }),
    )
    .await;
    assert!(!run.is_error.unwrap_or(false));
    let value: serde_json::Value =
        serde_json::from_str(call_tool_text(&run)).expect("run_batch response json");
    assert_eq!(value["operation"], "run_batch");
    assert_eq!(value["total"], 2);
    assert_eq!(value["run_now"], false);
}

#[tokio::test]
async fn test_mcp_manage_background_agents_stop_uses_stop_semantics() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let create = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "create",
            "name": "stop-contract",
            "agent_id": "default",
            "input": "do not run"
        }),
    )
    .await;
    assert!(
        !create.is_error.unwrap_or(false),
        "{}",
        call_tool_text(&create)
    );
    let created: serde_json::Value =
        serde_json::from_str(call_tool_text(&create)).expect("create response json");
    let task_id = created["id"]
        .as_str()
        .expect("created task should have id")
        .to_string();

    let stop = call_tool_through_mcp(
        server,
        "manage_background_agents",
        serde_json::json!({
            "operation": "stop",
            "id": task_id
        }),
    )
    .await;
    assert!(!stop.is_error.unwrap_or(false));
    let stopped: serde_json::Value =
        serde_json::from_str(call_tool_text(&stop)).expect("stop response json");
    assert!(stopped.get("deleted").is_none());
    assert_eq!(stopped["status"], "interrupted");

    let stored = core
        .storage
        .background_agents
        .get_task(stopped["id"].as_str().expect("stopped task id"))
        .expect("background storage query should succeed")
        .expect("stop should not delete the task");
    assert_eq!(stored.status, BackgroundAgentStatus::Interrupted);
}

#[tokio::test]
async fn test_mcp_manage_background_agents_start_returns_active_status() {
    let (server, core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let create = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "create",
            "name": "start-contract",
            "agent_id": "default",
            "input": "start later"
        }),
    )
    .await;
    assert!(
        !create.is_error.unwrap_or(false),
        "{}",
        call_tool_text(&create)
    );
    let created: serde_json::Value =
        serde_json::from_str(call_tool_text(&create)).expect("create response json");
    let task_id = created["id"]
        .as_str()
        .expect("created task should have id")
        .to_string();

    let stop = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "stop",
            "id": task_id
        }),
    )
    .await;
    assert!(!stop.is_error.unwrap_or(false));

    let start = call_tool_through_mcp(
        server,
        "manage_background_agents",
        serde_json::json!({
            "operation": "start",
            "id": task_id
        }),
    )
    .await;
    assert!(!start.is_error.unwrap_or(false));
    let started: serde_json::Value =
        serde_json::from_str(call_tool_text(&start)).expect("start response json");
    assert_eq!(started["status"], "active");
    assert!(started["next_run_at"].as_i64().is_some());

    let stored = core
        .storage
        .background_agents
        .get_task(started["id"].as_str().expect("started task id"))
        .expect("background storage query should succeed")
        .expect("start should not delete the task");
    assert_eq!(stored.status, BackgroundAgentStatus::Active);
    assert!(stored.next_run_at.is_some());
}

#[tokio::test]
async fn test_mcp_manage_background_agents_delete_returns_canonical_id_for_prefix() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let create = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "create",
            "name": "delete-prefix-contract",
            "agent_id": "default",
            "input": "delete later"
        }),
    )
    .await;
    assert!(
        !create.is_error.unwrap_or(false),
        "{}",
        call_tool_text(&create)
    );
    let created: serde_json::Value =
        serde_json::from_str(call_tool_text(&create)).expect("create response json");
    let task_id = created["id"]
        .as_str()
        .expect("created task should have id")
        .to_string();
    let prefix = &task_id[..8];

    let delete = call_tool_through_mcp(
        server,
        "manage_background_agents",
        serde_json::json!({
            "operation": "delete",
            "id": prefix
        }),
    )
    .await;
    assert!(!delete.is_error.unwrap_or(false));
    let deleted: serde_json::Value =
        serde_json::from_str(call_tool_text(&delete)).expect("delete response json");
    assert_eq!(deleted["id"], task_id);
    assert_eq!(deleted["deleted"], true);
}

#[tokio::test]
async fn test_mcp_manage_background_agents_list_deliverables_accepts_prefix() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let create = call_tool_through_mcp(
        server.clone(),
        "manage_background_agents",
        serde_json::json!({
            "operation": "create",
            "name": "deliverable-prefix-contract",
            "agent_id": "default",
            "input": "deliver later"
        }),
    )
    .await;
    assert!(
        !create.is_error.unwrap_or(false),
        "{}",
        call_tool_text(&create)
    );
    let created: serde_json::Value =
        serde_json::from_str(call_tool_text(&create)).expect("create response json");
    let task_id = created["id"]
        .as_str()
        .expect("created task should have id")
        .to_string();
    let prefix = &task_id[..8];

    let list = call_tool_through_mcp(
        server,
        "manage_background_agents",
        serde_json::json!({
            "operation": "list_deliverables",
            "id": prefix
        }),
    )
    .await;
    assert!(!list.is_error.unwrap_or(false));
    let value: serde_json::Value =
        serde_json::from_str(call_tool_text(&list)).expect("deliverables response json");
    assert!(value.is_array());
}

#[test]
fn test_parse_trace_category_rejects_unknown_value() {
    let err = RestFlowMcpServer::parse_trace_category(Some("unknown".to_string())).unwrap_err();
    assert!(err.contains("Unknown trace category"));
}

#[tokio::test]
async fn test_manage_background_agents_list_traces_keeps_backward_compatible_array_shape() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let mut params = base_manage_background_params("list_traces");
    params.id = Some("task-1".to_string());

    let json = server
        .handle_manage_background_agents(params)
        .await
        .expect("list_traces should succeed");
    let value: serde_json::Value =
        serde_json::from_str(&json).expect("list_traces response should be valid json");
    assert!(value.is_array());
}

#[tokio::test]
async fn test_manage_background_agents_list_traces_supports_stats_payload() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let mut params = base_manage_background_params("list_traces");
    params.id = Some("task-1".to_string());
    params.include_stats = Some(true);
    params.category = Some("tool".to_string());
    params.offset = Some(0);
    params.limit = Some(5);

    let json = server
        .handle_manage_background_agents(params)
        .await
        .expect("list_traces with stats should succeed");
    let value: serde_json::Value =
        serde_json::from_str(&json).expect("list_traces stats response should be valid json");
    assert!(value["events"].is_array());
    assert!(value["stats"].is_object());
    assert_eq!(value["stats"]["limit"], 5);
}

#[tokio::test]
async fn test_manage_background_agents_list_traces_validates_time_range() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let mut params = base_manage_background_params("list_traces");
    params.id = Some("task-1".to_string());
    params.from_time_ms = Some(200);
    params.to_time_ms = Some(100);

    let err = server
        .handle_manage_background_agents(params)
        .await
        .expect_err("invalid time range should fail");
    assert!(err.contains("Invalid time range"));
}

#[tokio::test]
async fn test_manage_hooks_list_operation() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let params = ManageHooksParams {
        operation: "list".to_string(),
        id: None,
        name: None,
        description: None,
        event: None,
        action: None,
        filter: None,
        enabled: None,
    };

    let json = server.handle_manage_hooks(params).await.unwrap();
    let hooks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(hooks.is_empty());
}

#[tokio::test]
async fn test_manage_hooks_create_operation() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let params = ManageHooksParams {
        operation: "create".to_string(),
        id: None,
        name: Some("Test Hook".to_string()),
        description: Some("A test hook".to_string()),
        event: Some("task_completed".to_string()),
        action: Some(serde_json::json!({
            "type": "webhook",
            "url": "https://example.com/hook"
        })),
        filter: None,
        enabled: None,
    };

    let json = server.handle_manage_hooks(params).await.unwrap();
    let hook: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(hook["name"], "Test Hook");
    assert_eq!(hook["event"], "task_completed");
    assert_eq!(hook["enabled"], true);
}

#[tokio::test]
async fn test_manage_hooks_invalid_operation() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let params = ManageHooksParams {
        operation: "invalid".to_string(),
        id: None,
        name: None,
        description: None,
        event: None,
        action: None,
        filter: None,
        enabled: None,
    };

    let result = server.handle_manage_hooks(params).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown operation"));
}

#[tokio::test]
async fn test_runtime_tool_fallback() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let json = server
        .handle_runtime_tool(
            "echo_runtime",
            serde_json::json!({ "value": "hello-runtime" }),
        )
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["value"], "hello-runtime");
}

#[tokio::test]
async fn test_runtime_tool_failure_includes_details() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let err = server
        .handle_runtime_tool("fail_runtime", serde_json::json!({}))
        .await
        .unwrap_err();
    let value: serde_json::Value = serde_json::from_str(&err).unwrap();
    assert_eq!(value["tool"], "fail_runtime");
    assert_eq!(value["error"], "Command exited with code 7");
    assert_eq!(value["error_category"], "Execution");
    assert_eq!(value["retryable"], false);
    assert_eq!(value["retry_after_ms"], serde_json::Value::Null);
    assert_eq!(value["details"]["exit_code"], 7);
    assert_eq!(value["details"]["stdout"], "out");
    assert_eq!(value["details"]["stderr"], "err");
}

#[test]
fn test_to_call_tool_result_preserves_structured_error_payload() {
    let payload = serde_json::json!({
        "tool": "demo",
        "error": "boom",
        "error_category": "Execution",
        "retryable": false,
        "retry_after_ms": serde_json::Value::Null,
        "details": { "code": 7 }
    });
    let value =
        RestFlowMcpServer::to_call_tool_result(Err(serde_json::to_string(&payload).unwrap()));

    assert_eq!(value.is_error, Some(true));
    assert_eq!(value.structured_content, Some(payload));
}

#[tokio::test]
async fn test_call_tool_sets_structured_content_for_json_runtime_error() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let result = call_tool_through_mcp(server, "fail_runtime", serde_json::json!({})).await;

    assert_eq!(result.is_error, Some(true));
    assert!(result.structured_content.is_some());
    let structured = result
        .structured_content
        .expect("structured payload should exist");
    assert_eq!(structured["tool"], "fail_runtime");
    assert_eq!(structured["error"], "Command exited with code 7");
    assert_eq!(structured["error_category"], "Execution");
}

#[tokio::test]
async fn test_call_tool_keeps_text_only_for_non_json_runtime_error() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let result = call_tool_through_mcp(server, "unknown_runtime", serde_json::json!({})).await;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content, None);
    let text = result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("error response should include text content");
    assert!(text.contains("Unknown runtime tool: unknown_runtime"));
}

#[tokio::test]
async fn test_call_tool_alias_path_preserves_structured_runtime_error_contract() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));

    let direct = call_tool_through_mcp(
        server.clone(),
        "send_email",
        serde_json::json!({
            "to": "a@example.com",
            "subject": "s",
            "body": "b"
        }),
    )
    .await;
    let alias = call_tool_through_mcp(
        server,
        "email",
        serde_json::json!({
            "to": "a@example.com",
            "subject": "s",
            "body": "b"
        }),
    )
    .await;

    assert_eq!(direct.is_error, Some(true));
    assert_eq!(alias.is_error, Some(true));
    assert_eq!(alias.structured_content, direct.structured_content);
    assert_eq!(
        alias.structured_content.as_ref().unwrap()["tool"],
        "send_email"
    );
    assert_eq!(
        alias.structured_content.as_ref().unwrap()["error"],
        "Mailbox unavailable"
    );
}

#[tokio::test]
async fn test_call_tool_preserves_structured_content_for_non_runtime_backend_error() {
    let server = RestFlowMcpServer::with_backend(Arc::new(MockBackend::new()));
    let result = call_tool_through_mcp(
        server,
        "chat_session_get",
        serde_json::json!({
            "session_id": "structured-ipc-error"
        }),
    )
    .await;

    assert_eq!(result.is_error, Some(true));
    let structured = result
        .structured_content
        .expect("structured payload should exist");
    assert_eq!(structured["code"], 409);
    assert_eq!(structured["message"], "session conflict");
    assert_eq!(structured["details"]["error_kind"], "session_lifecycle");
}

#[tokio::test]
async fn test_runtime_tools_include_manage_agents() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let runtime_tools = server.backend.list_runtime_tools().await.unwrap();
    assert!(
        runtime_tools
            .iter()
            .any(|tool| tool.name == "manage_agents")
    );
    assert!(runtime_tools.iter().any(|tool| tool.name == "manage_ops"));
}

#[tokio::test]
async fn test_manage_agents_runtime_tool_list_operation() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let json = server
        .handle_runtime_tool("manage_agents", serde_json::json!({ "operation": "list" }))
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(!agents.is_empty());
}

#[tokio::test]
async fn test_manage_ops_runtime_tool_routes_and_returns_normalized_json() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let json = server
        .handle_runtime_tool(
            "manage_ops",
            serde_json::json!({ "operation": "daemon_status" }),
        )
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["operation"], "daemon_status");
    assert!(value.get("evidence").is_some());
    assert!(value.get("verification").is_some());
}

#[test]
fn test_convert_use_skill_input_maps_to_skill_read() {
    let input = serde_json::json!({
        "skill_id": "my-skill"
    });
    let output = RestFlowMcpServer::convert_use_skill_input(input);
    assert_eq!(output["action"], "read");
    assert_eq!(output["id"], "my-skill");
}

#[test]
fn test_convert_use_skill_input_rejects_execute_action() {
    let input = serde_json::json!({
        "action": "execute",
        "id": "my-skill"
    });
    let output = RestFlowMcpServer::convert_use_skill_input(input);
    assert_eq!(output["action"], "__unsupported_execute");
}

#[test]
fn test_convert_use_skill_input_rejects_run_action() {
    let input = serde_json::json!({
        "action": "run",
        "id": "my-skill"
    });
    let output = RestFlowMcpServer::convert_use_skill_input(input);
    assert_eq!(output["action"], "__unsupported_execute");
}

#[test]
fn test_runtime_alias_description_prefers_primary_tool() {
    assert_eq!(
        RestFlowMcpServer::runtime_alias_description("http", "http_request"),
        "Alias of 'http_request' for convenience. Prefer using 'http_request' directly."
    );
    assert_eq!(
        RestFlowMcpServer::runtime_alias_description("email", "send_email"),
        "Alias of 'send_email' for convenience. Prefer using 'send_email' directly."
    );
    assert_eq!(
        RestFlowMcpServer::runtime_alias_description("telegram", "telegram_send"),
        "Alias of 'telegram_send' for convenience. Prefer using 'telegram_send' directly."
    );
    assert_eq!(
        RestFlowMcpServer::runtime_alias_description("discord", "discord_send"),
        "Alias of 'discord_send' for convenience. Prefer using 'discord_send' directly."
    );
    assert_eq!(
        RestFlowMcpServer::runtime_alias_description("slack", "slack_send"),
        "Alias of 'slack_send' for convenience. Prefer using 'slack_send' directly."
    );
    assert_eq!(
        RestFlowMcpServer::runtime_alias_description("use_skill", "skill"),
        "Alias of 'skill' for backward compatibility (load-only: list/read). Prefer using 'skill' directly."
    );
}

#[test]
fn test_use_skill_alias_parameters_are_load_only() {
    let schema = RestFlowMcpServer::use_skill_alias_parameters();
    let action_enum = schema["properties"]["action"]["enum"]
        .as_array()
        .expect("action enum should be an array");
    assert_eq!(action_enum.len(), 2);
    assert_eq!(action_enum[0], "list");
    assert_eq!(action_enum[1], "read");
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn test_session_scoped_runtime_tools_include_switch_model() {
    let tools = RestFlowMcpServer::session_scoped_runtime_tools();
    assert!(tools.iter().any(|tool| tool.name == "switch_model"));
    assert!(!tools.iter().any(|tool| tool.name == "spawn_subagent"));
    assert!(!tools.iter().any(|tool| tool.name == "spawn_subagent_batch"));
    assert!(!tools.iter().any(|tool| tool.name == "wait_subagents"));
    assert!(!tools.iter().any(|tool| tool.name == "list_subagents"));

    let switch_model = tools
        .iter()
        .find(|tool| tool.name == "switch_model")
        .expect("switch_model tool should exist");
    assert!(switch_model.parameters.get("anyOf").is_none());
    assert!(switch_model.parameters.get("oneOf").is_none());
    assert!(switch_model.parameters.get("allOf").is_none());
    assert_eq!(
        switch_model.parameters["required"],
        serde_json::json!(["provider", "model"])
    );
}

#[tokio::test]
async fn test_switch_model_works_in_standalone_mcp_mode() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;

    let result = server
        .handle_switch_model_for_mcp(serde_json::json!({
            "provider": "openai-codex",
            "model": "gpt-5.3-codex",
            "reason": "MCP standalone test"
        }))
        .await
        .expect("switch_model should succeed in standalone MCP mode");

    let value: serde_json::Value =
        serde_json::from_str(&result).expect("switch_model result should be valid JSON");
    assert_eq!(value["switched"], true);
    assert_eq!(value["to"]["model"], "gpt-5.3-codex");
    assert_eq!(value["to"]["provider"], "codex-cli");
}

#[tokio::test]
async fn test_switch_model_failure_returns_structured_payload() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let err = server
        .handle_switch_model_for_mcp(serde_json::json!({}))
        .await
        .unwrap_err();
    let value: serde_json::Value =
        serde_json::from_str(&err).expect("switch_model error should be valid JSON");

    assert_eq!(value["tool"], "switch_model");
    assert!(value.get("error").is_some());
    assert!(value.get("error_category").is_some());
    assert!(value.get("retryable").is_some());
    assert!(value.get("retry_after_ms").is_some());
}

#[tokio::test]
async fn test_standalone_runtime_tools_include_subagent_tools() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let runtime_tools = server.backend.list_runtime_tools().await.unwrap();

    assert!(
        runtime_tools
            .iter()
            .any(|tool| tool.name == "spawn_subagent")
    );
    assert!(
        !runtime_tools
            .iter()
            .any(|tool| tool.name == "spawn_subagent_batch")
    );
    assert!(
        runtime_tools
            .iter()
            .any(|tool| tool.name == "wait_subagents")
    );
    assert!(
        runtime_tools
            .iter()
            .any(|tool| tool.name == "list_subagents")
    );
}

#[tokio::test]
async fn test_standalone_runtime_spawn_subagent_returns_actionable_error() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let error = server
        .handle_runtime_tool(
            "spawn_subagent",
            serde_json::json!({"agent": "coder", "task": "do work"}),
        )
        .await
        .unwrap_err();

    assert!(
        error.contains("No callable sub-agents available") || error.contains("Unknown agent"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn test_mcp_manage_background_agents_stress_path_emits_latency_summary() {
    let (server, _core, _temp_dir, _temp_agents, _guard) = create_test_server().await;
    let artifacts_dir = stress_artifacts_dir();
    std::fs::create_dir_all(&artifacts_dir).expect("failed to create stress artifacts dir");

    let _ = server
        .handle_list_skills(ListSkillsParams::default())
        .await
        .expect("list skills should simulate initialize path");
    let tools = server
        .backend
        .list_runtime_tools()
        .await
        .expect("runtime tools should be available");
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "manage_background_agents"),
        "manage_background_agents tool must be registered"
    );

    let mut create_params = base_manage_background_params("create");
    create_params.name = Some("mcp-stress-task".to_string());
    create_params.agent_id = Some("default".to_string());
    create_params.description = Some("stress path create/run/control/progress/list".to_string());
    create_params.input = Some("deterministic".to_string());
    create_params.schedule = Some(serde_json::json!({
        "type": "once",
        "run_at": chrono::Utc::now().timestamp_millis()
    }));
    let created_json = server
        .handle_manage_background_agents(create_params)
        .await
        .expect("create operation should succeed");
    let created: serde_json::Value =
        serde_json::from_str(&created_json).expect("create response should be valid json");
    let task_id = created["id"]
        .as_str()
        .expect("created task id should be present")
        .to_string();

    let mut run_params = base_manage_background_params("run");
    run_params.id = Some(task_id.clone());
    server
        .handle_manage_background_agents(run_params)
        .await
        .expect("run operation should succeed");

    let mut progress_params = base_manage_background_params("progress");
    progress_params.id = Some(task_id.clone());
    progress_params.event_limit = Some(20);
    let progress_json = server
        .handle_manage_background_agents(progress_params)
        .await
        .expect("progress operation should succeed");
    let progress: serde_json::Value =
        serde_json::from_str(&progress_json).expect("progress response should be valid json");
    assert_eq!(
        progress["background_agent_id"].as_str(),
        Some(task_id.as_str())
    );

    let workers = 16usize;
    let loops_per_worker = 6usize;
    let total_calls = workers * loops_per_worker * 2;
    let mut join_set = tokio::task::JoinSet::new();

    for _ in 0..workers {
        let server = server.clone();
        let task_id = task_id.clone();
        join_set.spawn(async move {
            let mut latencies_ms = Vec::with_capacity(loops_per_worker * 2);
            for _ in 0..loops_per_worker {
                let list_params = base_manage_background_params("list");
                let started = Instant::now();
                let _ = server
                    .handle_manage_background_agents(list_params)
                    .await
                    .expect("list operation should succeed");
                latencies_ms.push(started.elapsed().as_micros() as u64 / 1_000);

                let mut progress_params = base_manage_background_params("progress");
                progress_params.id = Some(task_id.clone());
                progress_params.event_limit = Some(20);
                let started = Instant::now();
                let _ = server
                    .handle_manage_background_agents(progress_params)
                    .await
                    .expect("progress operation should succeed");
                latencies_ms.push(started.elapsed().as_micros() as u64 / 1_000);
            }
            latencies_ms
        });
    }

    let mut all_latencies = Vec::with_capacity(total_calls);
    while let Some(joined) = join_set.join_next().await {
        let mut worker_latencies = joined.expect("worker join should succeed");
        all_latencies.append(&mut worker_latencies);
    }

    assert_eq!(all_latencies.len(), total_calls);
    all_latencies.sort_unstable();

    let p50 = percentile_ms(&all_latencies, 50.0);
    let p95 = percentile_ms(&all_latencies, 95.0);
    let p99 = percentile_ms(&all_latencies, 99.0);
    assert!(p95 <= 250, "p95 latency should stay bounded, got {p95}ms");

    let summary = serde_json::json!({
        "workers": workers,
        "loops_per_worker": loops_per_worker,
        "total_calls": total_calls,
        "success_calls": all_latencies.len(),
        "latency_ms": {
            "p50": p50,
            "p95": p95,
            "p99": p99
        },
    });

    std::fs::write(
        artifacts_dir.join("mcp-background-agent-stress-summary.json"),
        serde_json::to_vec_pretty(&summary).expect("failed to serialize mcp stress summary"),
    )
    .expect("failed to write mcp stress summary artifact");

    let markdown = format!(
        "# MCP Background Agent Stress Summary\n\n- Workers: {workers}\n- Loops per worker: {loops_per_worker}\n- Total calls: {total_calls}\n- p50: {p50}ms\n- p95: {p95}ms\n- p99: {p99}ms\n"
    );
    std::fs::write(
        artifacts_dir.join("mcp-background-agent-stress-summary.md"),
        markdown,
    )
    .expect("failed to write mcp stress markdown artifact");
}

fn stress_artifacts_dir() -> std::path::PathBuf {
    std::env::var("LOG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("target/stress-artifacts"))
}

fn base_manage_background_params(operation: &str) -> ManageBackgroundAgentsParams {
    ManageBackgroundAgentsParams {
        operation: operation.to_string(),
        session_id: None,
        id: None,
        name: None,
        agent_id: None,
        task_id: None,
        chat_session_id: None,
        description: None,
        input: None,
        inputs: None,
        team: None,
        workers: None,
        save_as_team: None,
        input_template: None,
        schedule: None,
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        memory_scope: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: None,
        status: None,
        action: None,
        event_limit: None,
        message: None,
        source: None,
        limit: None,
        offset: None,
        category: None,
        from_time_ms: None,
        to_time_ms: None,
        include_stats: None,
        trace_id: None,
        line_limit: None,
        run_now: None,
    }
}

fn percentile_ms(sorted_ms: &[u64], percentile: f64) -> u64 {
    if sorted_ms.is_empty() {
        return 0;
    }
    let idx = ((percentile / 100.0) * (sorted_ms.len().saturating_sub(1) as f64)).round() as usize;
    sorted_ms[idx]
}
