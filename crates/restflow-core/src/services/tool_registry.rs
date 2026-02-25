//! Tool registry service for creating tool registries with storage access.
//!
//! Adapter implementations live in [`super::adapters`]. This module provides
//! the [`create_tool_registry`] function that wires adapters into tools.

use crate::lsp::LspManager;
use crate::memory::UnifiedSearchEngine;
use crate::services::adapters::*;
use crate::storage::skill::SkillStorage;
use crate::storage::{
    AgentStorage, BackgroundAgentStorage, ChatSessionStorage, ConfigStorage, KvStoreStorage,
    MemoryStorage, SecretStorage, TerminalSessionStorage, TriggerStorage, WorkItemStorage,
};
use restflow_tools::ToolRegistryBuilder;
use restflow_traits::registry::ToolRegistry;
use restflow_traits::tool::SecretResolver;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Create a tool registry with all available tools including storage-backed tools.
///
/// This function creates a registry with:
/// - Default tools from restflow-ai (http_request, send_email)
/// - SkillTool that can access skills from storage
/// - Memory search tool for unified memory and session search
/// - Agent memory CRUD tools (save_to_memory, read_memory, etc.) — always registered, agent_id is a tool input
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    kv_store_storage: KvStoreStorage,
    work_item_storage: WorkItemStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_storage: AgentStorage,
    background_agent_storage: BackgroundAgentStorage,
    trigger_storage: TriggerStorage,
    terminal_storage: TerminalSessionStorage,
    deliverable_storage: crate::storage::DeliverableStorage,
    accessor_id: Option<String>,
    _agent_id: Option<String>,
) -> anyhow::Result<ToolRegistry> {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let lsp_manager = Arc::new(LspManager::new(root));

    let secret_resolver: SecretResolver = {
        let secrets = Arc::new(secret_storage.clone());
        Arc::new(move |key| secrets.get_secret(key).ok().flatten())
    };

    // Create adapters
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage.clone()));
    let session_store = Arc::new(SessionStorageAdapter::new(
        chat_storage.clone(),
        agent_storage.clone(),
    ));
    let memory_manager = Arc::new(MemoryManagerAdapter::new(memory_storage.clone()));
    let mem_store = Arc::new(DbMemoryStoreAdapter::new(memory_storage.clone()));
    let deliverable_store = Arc::new(DeliverableStoreAdapter::new(deliverable_storage.clone()));
    let search_engine = UnifiedSearchEngine::new(memory_storage, chat_storage.clone());
    let unified_search = Arc::new(UnifiedMemorySearchAdapter::new(search_engine));
    let ops_provider = Arc::new(OpsProviderAdapter::new(
        background_agent_storage.clone(),
        chat_storage,
    ));
    let kv_store = Arc::new(KvStoreAdapter::new(kv_store_storage, accessor_id));
    let work_item_provider = Arc::new(DbWorkItemAdapter::new(work_item_storage));
    let auth_store = Arc::new(AuthProfileStorageAdapter::new(secret_storage.clone()));
    let known_tools = Arc::new(RwLock::new(HashSet::new()));
    let agent_store = Arc::new(AgentStoreAdapter::new(
        agent_storage.clone(),
        skill_storage.clone(),
        secret_storage.clone(),
        background_agent_storage.clone(),
        known_tools.clone(),
    ));
    let background_agent_store = Arc::new(BackgroundAgentStoreAdapter::new(
        background_agent_storage,
        agent_storage,
        deliverable_storage,
    ));
    let marketplace_store = Arc::new(MarketplaceStoreAdapter::new(skill_storage));
    let trigger_store = Arc::new(TriggerStoreAdapter::new(trigger_storage));
    let terminal_store = Arc::new(TerminalStoreAdapter::new(terminal_storage));
    let security_provider: Arc<_> = Arc::new(SecurityQueryProviderAdapter);

    let registry = ToolRegistryBuilder::new()
        .with_bash(restflow_tools::BashConfig::default())
        .with_file(restflow_tools::FileConfig::default())
        .with_http()?
        .with_email()
        .with_telegram()?
        .with_discord()?
        .with_slack()?
        .with_python()
        .with_web_fetch()
        .with_jina_reader()?
        .with_web_search_with_resolver(secret_resolver.clone())?
        .with_diagnostics(lsp_manager)
        .with_transcribe(secret_resolver.clone())?
        .with_vision(secret_resolver)?
        .with_skill_tool(skill_provider)
        .with_session(session_store)
        .with_memory_management(memory_manager)
        .with_memory_store(mem_store)
        .with_deliverable(deliverable_store)
        .with_unified_search(unified_search)
        .with_ops(ops_provider)
        .with_kv_store(kv_store)
        .with_work_items(work_item_provider)
        .with_auth_profile(auth_store)
        .with_secrets(Arc::new(secret_storage))
        .with_config(Arc::new(config_storage))
        .with_agent_crud(agent_store)
        .with_background_agent(background_agent_store)
        .with_marketplace(marketplace_store)
        .with_trigger(trigger_store)
        .with_terminal(terminal_store)
        .with_security_query(security_provider)
        .build();

    // Populate known_tools for AgentStoreAdapter validation
    if let Ok(mut known) = known_tools.write() {
        *known = registry
            .list()
            .into_iter()
            .map(|name| name.to_string())
            .collect::<HashSet<_>>();
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Skill;
    use crate::services::adapters::{
        AgentStoreAdapter, BackgroundAgentStoreAdapter, DbMemoryStoreAdapter, OpsProviderAdapter,
    };
    use redb::Database;
    use restflow_traits::skill::SkillProvider as _;
    use restflow_traits::store::{
        AgentCreateRequest, AgentStore, AgentUpdateRequest, BackgroundAgentControlRequest,
        BackgroundAgentCreateRequest, BackgroundAgentMessageListRequest,
        BackgroundAgentMessageRequest, BackgroundAgentProgressRequest,
        BackgroundAgentScratchpadListRequest, BackgroundAgentScratchpadReadRequest,
        BackgroundAgentStore, BackgroundAgentUpdateRequest, MemoryStore as _,
    };
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn restflow_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[allow(clippy::type_complexity)]
    fn setup_storage() -> (
        SkillStorage,
        MemoryStorage,
        ChatSessionStorage,
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
        let _restflow_env_lock = restflow_dir_env_lock();

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

    #[test]
    fn test_create_tool_registry() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
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
        )
        .unwrap();

        // Should have default tools + skill tool
        assert!(registry.has("http_request"));
        assert!(registry.has("send_email"));
        assert!(registry.has("skill"));
        assert!(registry.has("memory_search"));
        assert!(registry.has("kv_store"));
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
    async fn test_manage_ops_session_summary_response_schema() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
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
        let _lock = restflow_dir_env_lock();
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
        let _lock = restflow_dir_env_lock();
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
        let _lock = restflow_dir_env_lock();
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

    #[test]
    fn test_background_agent_scratchpad_rejects_symlink_path() {
        let _lock = restflow_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

        let scratchpads_dir = crate::paths::resolve_restflow_dir()
            .unwrap()
            .join("scratchpads");
        std::fs::create_dir_all(&scratchpads_dir).unwrap();

        let outside_file = temp_dir.path().join("outside.jsonl");
        std::fs::write(&outside_file, "sensitive data").unwrap();

        let symlink_path = scratchpads_dir.join("malicious.jsonl");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        let _request = BackgroundAgentScratchpadReadRequest {
            scratchpad: "malicious.jsonl".to_string(),
            line_limit: Some(10),
        };

        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var("RESTFLOW_DIR", value);
            } else {
                std::env::remove_var("RESTFLOW_DIR");
            }
        }

        let result = BackgroundAgentStoreAdapter::validate_scratchpad_name("malicious.jsonl");
        assert!(result.is_ok(), "validation should accept the filename");
    }

    #[test]
    fn test_background_agent_scratchpad_path_escape() {
        let _lock = restflow_dir_env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", &state_dir) };

        let scratchpads_dir = crate::paths::resolve_restflow_dir()
            .unwrap()
            .join("scratchpads");
        std::fs::create_dir_all(&scratchpads_dir).unwrap();

        let attack_dir = scratchpads_dir.join("attack");
        std::fs::create_dir_all(&attack_dir).unwrap();

        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var("RESTFLOW_DIR", value);
            } else {
                std::env::remove_var("RESTFLOW_DIR");
            }
        }

        let result = BackgroundAgentStoreAdapter::validate_scratchpad_name("../etc/passwd.jsonl");
        assert!(result.is_err(), "validation should reject path traversal");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("invalid"),
            "error should mention invalid name"
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
        let audit_skill = crate::models::Skill::new(
            "audit-skill".to_string(),
            "Audit Skill".to_string(),
            None,
            None,
            "audit".to_string(),
        );
        skill_storage.create(&audit_skill).unwrap();

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
            model: Some(crate::models::AIModel::ClaudeSonnet4_5),
            prompt: Some("You are a testing assistant".to_string()),
            temperature: Some(0.3),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(crate::models::ApiKeyConfig::Direct("test-key".to_string())),
            tools: Some(vec!["manage_background_agents".to_string()]),
            skills: Some(vec!["ops-skill".to_string()]),
            skill_variables: None,
            model_routing: None,
        };

        let created = AgentStore::create_agent(
            &adapter,
            AgentCreateRequest {
                name: "Ops Agent".to_string(),
                agent: serde_json::to_value(base_node).unwrap(),
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
                agent: Some(serde_json::json!({
                    "model": "gpt-5-mini",
                    "prompt": "Updated prompt",
                    "tools": ["manage_background_agents", "manage_agents"],
                    "skills": ["ops-skill", "audit-skill"]
                })),
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
                agent: serde_json::json!({
                    "tools": ["unknown_tool"]
                }),
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
                agent: serde_json::json!({
                    "model": "claude-sonnet-4-5",
                    "prompt": "owner"
                }),
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
            _memory_storage,
            _chat_storage,
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
            background_agent_storage,
            agent_storage,
            deliverable_storage,
        );

        let created = BackgroundAgentStore::create_background_agent(
            &adapter,
            BackgroundAgentCreateRequest {
                name: "Background Agent".to_string(),
                agent_id: created_agent.id,
                chat_session_id: None,
                schedule: None,
                input: Some("Run periodic checks".to_string()),
                input_template: Some("Template {{task.id}}".to_string()),
                timeout_secs: Some(1800),
                durability_mode: Some("async".to_string()),
                memory: None,
                memory_scope: Some("per_background_agent".to_string()),
                resource_limits: None,
            },
        )
        .unwrap();
        assert_eq!(
            created
                .get("input_template")
                .and_then(|value| value.as_str()),
            Some("Template {{task.id}}")
        );
        assert_eq!(
            created
                .get("memory")
                .and_then(|value| value.get("memory_scope"))
                .and_then(|value| value.as_str()),
            Some("per_background_agent")
        );
        let task_id = created
            .get("id")
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
                durability_mode: Some("sync".to_string()),
                memory: None,
                memory_scope: Some("shared_agent".to_string()),
                resource_limits: None,
            },
        )
        .unwrap();
        assert_eq!(
            updated.get("name").and_then(|value| value.as_str()),
            Some("Background Agent Updated")
        );
        assert_eq!(
            updated
                .get("memory")
                .and_then(|value| value.get("memory_scope"))
                .and_then(|value| value.as_str()),
            Some("shared_agent")
        );
        assert_eq!(
            updated.get("timeout_secs").and_then(|value| value.as_u64()),
            Some(900)
        );

        let controlled = BackgroundAgentStore::control_background_agent(
            &adapter,
            BackgroundAgentControlRequest {
                id: task_id.clone(),
                action: "run_now".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            controlled.get("status").and_then(|value| value.as_str()),
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

        let _restflow_lock = restflow_dir_env_lock();
        let scratchpad_state = tempdir().unwrap();
        let previous_restflow_dir = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", scratchpad_state.path()) };

        let scratchpad_dir = crate::paths::ensure_restflow_dir()
            .unwrap()
            .join("scratchpads");
        std::fs::create_dir_all(&scratchpad_dir).unwrap();
        let scratchpad_name = format!("{task_id}-20260214-120000.jsonl");
        std::fs::write(
            scratchpad_dir.join(&scratchpad_name),
            "{\"event_type\":\"execution_start\"}\n{\"event_type\":\"execution_complete\"}\n",
        )
        .unwrap();
        std::fs::write(
            scratchpad_dir.join("other-task-20260214-120000.jsonl"),
            "{\"event_type\":\"execution_start\"}\n",
        )
        .unwrap();

        let scratchpads = BackgroundAgentStore::list_background_agent_scratchpads(
            &adapter,
            BackgroundAgentScratchpadListRequest {
                id: Some(task_id.clone()),
                limit: Some(5),
            },
        )
        .unwrap();
        assert_eq!(scratchpads.as_array().map(|items| items.len()), Some(1));
        assert_eq!(
            scratchpads[0]
                .get("scratchpad")
                .and_then(|value| value.as_str()),
            Some(scratchpad_name.as_str())
        );

        let scratchpad_content = BackgroundAgentStore::read_background_agent_scratchpad(
            &adapter,
            BackgroundAgentScratchpadReadRequest {
                scratchpad: scratchpad_name,
                line_limit: Some(1),
            },
        )
        .unwrap();
        assert_eq!(scratchpad_content["total_lines"].as_u64(), Some(2));
        assert_eq!(
            scratchpad_content["lines"]
                .as_array()
                .map(|items| items.len()),
            Some(1)
        );

        unsafe {
            if let Some(value) = previous_restflow_dir {
                std::env::set_var("RESTFLOW_DIR", value);
            } else {
                std::env::remove_var("RESTFLOW_DIR");
            }
        }

        let deleted = BackgroundAgentStore::delete_background_agent(&adapter, &task_id).unwrap();
        assert_eq!(
            deleted.get("deleted").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_marketplace_tool_list_and_uninstall() {
        let (
            skill_storage,
            memory_storage,
            chat_storage,
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
        )
        .unwrap();

        assert!(registry.has("save_to_memory"));
        assert!(registry.has("read_memory"));
        assert!(registry.has("list_memories"));
        assert!(registry.has("delete_memory"));
    }

    #[test]
    fn test_validate_scratchpad_name_accepts_normal_file() {
        let result =
            BackgroundAgentStoreAdapter::validate_scratchpad_name("task-123-2026-02-18.jsonl");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_scratchpad_name_rejects_path_traversal() {
        let result = BackgroundAgentStoreAdapter::validate_scratchpad_name("../etc/passwd.jsonl");
        assert!(result.is_err());

        let result2 =
            BackgroundAgentStoreAdapter::validate_scratchpad_name("foo/../../../bar.jsonl");
        assert!(result2.is_err());

        let result3 = BackgroundAgentStoreAdapter::validate_scratchpad_name("foo\\bar.jsonl");
        assert!(result3.is_err());
    }
}
