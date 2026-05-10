//! Agent service layer
//!
//! This module only covers agent CRUD operations.
//! Agent execution happens through chat sessions and task runtime paths.

use crate::{
    AppCore,
    agent_validation::validate_agent_node_async,
    services::agent_catalog::{DEFAULT_ASSISTANT_NAME, StoredAgent},
    services::session::SessionService,
};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::sync::Arc;
use types::{AgentNode, ChatSessionSource, encode_validation_error};

pub async fn list_agents(core: &Arc<AppCore>) -> Result<Vec<StoredAgent>> {
    core.storage
        .agents
        .list_agents()
        .context("Failed to list agents")
}

pub async fn get_agent(core: &Arc<AppCore>, id: &str) -> Result<StoredAgent> {
    core.storage
        .agents
        .get_agent(id.to_string())
        .with_context(|| format!("Failed to get agent {}", id))?
        .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))
}

pub async fn create_agent(
    core: &Arc<AppCore>,
    name: String,
    mut agent: AgentNode,
) -> Result<StoredAgent> {
    normalize_model_fields(&mut agent)?;
    validate_agent_node(core, &agent).await?;
    core.storage
        .agents
        .create_agent(name.clone(), agent)
        .with_context(|| format!("Failed to create agent {}", name))
}

pub async fn update_agent(
    core: &Arc<AppCore>,
    id: &str,
    name: Option<String>,
    mut agent: Option<AgentNode>,
) -> Result<StoredAgent> {
    if let Some(agent_node) = agent.as_mut() {
        normalize_model_fields(agent_node)?;
        validate_agent_node(core, agent_node).await?;
    }
    core.storage
        .agents
        .update_agent(id.to_string(), name, agent)
        .with_context(|| format!("Failed to update agent {}", id))
}

/// Check whether an agent has managed chat sessions.
///
/// Returns `Ok(Some(source_list))` when linked managed sessions exist, `Ok(None)`
/// otherwise.
pub(crate) fn check_agent_has_managed_sessions(
    session_service: &SessionService,
    agent_id: &str,
) -> Result<Option<String>> {
    let sessions = session_service.list_session_views(Some(agent_id), None, true)?;
    let mut sources: BTreeSet<String> = BTreeSet::new();

    for session in sessions {
        if session.is_archived() {
            continue;
        }
        let (source, _) = session_service.effective_source(&session)?;
        if matches!(source, ChatSessionSource::Background) {
            sources.insert("background".to_string());
        }
    }

    if sources.is_empty() {
        Ok(None)
    } else {
        Ok(Some(sources.into_iter().collect::<Vec<_>>().join(", ")))
    }
}

pub async fn delete_agent(core: &Arc<AppCore>, id: &str) -> Result<()> {
    let resolved_id = core
        .storage
        .agents
        .resolve_existing_agent_id(id)
        .with_context(|| format!("Failed to resolve agent {}", id))?;

    let resolved_default_id = core.storage.agents.resolve_default_agent_id().ok();
    if resolved_default_id.as_deref() == Some(resolved_id.as_str()) {
        let agent_name = core
            .storage
            .agents
            .get_agent(resolved_id.clone())?
            .map(|agent| agent.name)
            .unwrap_or_else(|| DEFAULT_ASSISTANT_NAME.to_string());
        anyhow::bail!(
            "Cannot delete default assistant agent {} ({})",
            resolved_id,
            agent_name
        );
    }

    let session_service = SessionService::from_storage(&core.storage);
    if let Some(sources) = check_agent_has_managed_sessions(&session_service, &resolved_id)
        .with_context(|| format!("Failed to query chat sessions for agent {}", id))?
    {
        anyhow::bail!(
            "Cannot delete agent {}: managed sessions exist ({})",
            resolved_id,
            sources
        );
    }

    archive_agent_workspace_sessions(&session_service, &resolved_id).with_context(|| {
        format!(
            "Failed to archive workspace sessions before deleting agent {}",
            id
        )
    })?;

    core.storage
        .agents
        .delete_agent(resolved_id)
        .with_context(|| format!("Failed to delete agent {}", id))
}

fn normalize_model_fields(agent: &mut AgentNode) -> Result<()> {
    if let Err(error) = agent.normalize_model_fields() {
        anyhow::bail!(encode_validation_error(vec![error]));
    }
    Ok(())
}

fn archive_agent_workspace_sessions(
    session_service: &SessionService,
    agent_id: &str,
) -> Result<()> {
    for session in session_service.list_session_views(Some(agent_id), None, true)? {
        if session_service.management_owner(&session)?.is_none() {
            let _ = session_service.archive_session(&session.id)?;
        } else if session.source_channel == Some(ChatSessionSource::Background) {
            let _ = session_service.archive_managed_session(&session.id)?;
        }
    }
    Ok(())
}

async fn validate_agent_node(core: &Arc<AppCore>, agent: &AgentNode) -> Result<()> {
    if let Err(errors) = agent.validate() {
        anyhow::bail!(encode_validation_error(errors));
    }
    if let Err(errors) = validate_agent_node_async(agent, core).await {
        anyhow::bail!(encode_validation_error(errors));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use crate::prompt_files;
    use crate::time_utils;
    use tempfile::tempdir;
    use types::{ApiKeyConfig, ChatSession, ChatSessionSource, ModelId, ValidationErrorResponse};

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

    /// Create a test AppCore with an isolated agents directory.
    /// Returns (core, _temp_db_dir, _temp_agents_dir, _env_guard).
    /// All returned values must be held alive for the test duration.
    #[allow(clippy::await_holding_lock)]
    async fn create_test_core_isolated() -> (
        Arc<AppCore>,
        tempfile::TempDir,
        tempfile::TempDir,
        AgentsDirEnvGuard,
    ) {
        let env_guard = AgentsDirEnvGuard::new();
        let temp_db = tempdir().unwrap();
        let temp_agents = tempdir().unwrap();
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_agents.path()) };
        let db_path = temp_db.path().join("test.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (core, temp_db, temp_agents, env_guard)
    }

    #[test]
    fn test_agents_dir_env_guard_cleans_up_env_var() {
        let guard = AgentsDirEnvGuard::new();
        unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, "/tmp/restflow-test-agents") };
        drop(guard);
        assert!(std::env::var(prompt_files::AGENTS_DIR_ENV).is_err());
    }

    fn create_test_agent_node(prompt: &str) -> AgentNode {
        AgentNode {
            model_ref: Some(types::ModelRef::from_model(ModelId::ClaudeSonnet4_5)),
            prompt: Some(prompt.to_string()),
            temperature: Some(0.7),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["bash".to_string()]),
            skills: None,
            skill_variables: None,
            skill_preflight_policy_mode: None,
            model_routing: None,
        }
    }

    fn set_test_model(node: &mut AgentNode, model: ModelId) {
        node.model_ref = Some(types::ModelRef::from_model(model));
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let agents = list_agents(&core).await.unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "Default Assistant");
    }

    #[tokio::test]
    async fn test_create_and_get_agent() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("You are a helpful assistant");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        assert!(!created.id.is_empty());
        assert_eq!(created.name, "Test Agent");
        if let Some(prompt) = &created.agent.prompt {
            assert_eq!(prompt, "You are a helpful assistant");
        }

        let prompt_on_disk = prompt_files::load_agent_prompt_for_agent(
            &created.id,
            &created.name,
            created.prompt_file.as_deref(),
        )
        .unwrap();
        assert_eq!(
            prompt_on_disk.content,
            Some("You are a helpful assistant".to_string())
        );

        let retrieved = get_agent(&core, &created.id).await.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Test Agent");
        if let Some(prompt) = &retrieved.agent.prompt {
            assert_eq!(prompt, "You are a helpful assistant");
        }
    }

    #[tokio::test]
    async fn test_list_agents_multiple() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent1 = create_test_agent_node("Agent 1 prompt");
        let agent2 = create_test_agent_node("Agent 2 prompt");
        let agent3 = create_test_agent_node("Agent 3 prompt");

        create_agent(&core, "Agent 1".to_string(), agent1)
            .await
            .unwrap();
        create_agent(&core, "Agent 2".to_string(), agent2)
            .await
            .unwrap();
        create_agent(&core, "Agent 3".to_string(), agent3)
            .await
            .unwrap();

        let agents = list_agents(&core).await.unwrap();
        assert_eq!(agents.len(), 4);

        let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"Default Assistant".to_string()));
        assert!(names.contains(&"Agent 1".to_string()));
        assert!(names.contains(&"Agent 2".to_string()));
        assert!(names.contains(&"Agent 3".to_string()));
    }

    #[tokio::test]
    async fn test_update_agent_name() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Original Name".to_string(), agent_node)
            .await
            .unwrap();

        let updated = update_agent(&core, &created.id, Some("Updated Name".to_string()), None)
            .await
            .unwrap();

        assert_eq!(updated.name, "Updated Name");
        if let Some(prompt) = &updated.agent.prompt {
            assert_eq!(prompt, "Test prompt");
        }
        let prompt_on_disk = prompt_files::load_agent_prompt_for_agent(
            &updated.id,
            &updated.name,
            updated.prompt_file.as_deref(),
        )
        .unwrap();
        assert_eq!(prompt_on_disk.content, Some("Test prompt".to_string()));
    }

    #[tokio::test]
    async fn test_update_agent_config() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Original prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        // Use DeepseekChat which supports temperature (unlike Gpt5Mini)
        let mut new_agent_node = create_test_agent_node("Updated prompt");
        new_agent_node.temperature = Some(0.9);
        set_test_model(&mut new_agent_node, ModelId::DeepseekChat);

        let updated = update_agent(&core, &created.id, None, Some(new_agent_node))
            .await
            .unwrap();

        assert_eq!(updated.name, "Test Agent"); // Name unchanged
        if let Some(prompt) = &updated.agent.prompt {
            assert_eq!(prompt, "Updated prompt");
        }
        let prompt_on_disk = prompt_files::load_agent_prompt_for_agent(
            &updated.id,
            &updated.name,
            updated.prompt_file.as_deref(),
        )
        .unwrap();
        assert_eq!(prompt_on_disk.content, Some("Updated prompt".to_string()));
        assert_eq!(updated.agent.temperature, Some(0.9));
        assert_eq!(
            updated
                .agent
                .resolved_model_ref()
                .map(|model_ref| model_ref.model),
            Some(ModelId::DeepseekChat)
        );
    }

    #[tokio::test]
    async fn test_delete_agent() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "To Delete".to_string(), agent_node)
            .await
            .unwrap();

        // Verify it exists
        let retrieved = get_agent(&core, &created.id).await;
        assert!(retrieved.is_ok());

        // Delete it
        delete_agent(&core, &created.id).await.unwrap();

        // Verify it's gone
        let result = get_agent(&core, &created.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_default_assistant_is_blocked() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let default = core.storage.agents.resolve_default_agent().unwrap();
        assert!(default.name.eq_ignore_ascii_case(DEFAULT_ASSISTANT_NAME));

        let err = delete_agent(&core, &default.id).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Cannot delete default assistant agent"));
    }

    #[tokio::test]
    async fn test_delete_agent_archives_workspace_sessions() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Workspace owner");
        let created = create_agent(&core, "Workspace Owner".to_string(), agent_node)
            .await
            .unwrap();

        let mut session = ChatSession::new(
            created.id.clone(),
            ModelId::Gpt5.as_serialized_str().to_string(),
        )
        .with_name("Workspace Session");
        session.source_channel = Some(ChatSessionSource::Workspace);
        core.storage
            .file_sessions
            .write_session(
                &crate::session_log::FileSession::from_chat_session(&session),
                true,
            )
            .unwrap();

        delete_agent(&core, &created.id).await.unwrap();

        let active_sessions = core
            .storage
            .file_sessions
            .list()
            .unwrap()
            .into_iter()
            .map(|session| session.to_chat_session())
            .filter(|session| session.agent_id == created.id && !session.is_archived())
            .collect::<Vec<_>>();
        assert!(active_sessions.is_empty());

        let archived_session = core
            .storage
            .file_sessions
            .get(&session.id)
            .unwrap()
            .map(|session| session.to_chat_session())
            .expect("session should remain after archiving");
        assert!(archived_session.is_archived());
    }

    #[tokio::test]
    async fn test_delete_agent_blocks_orphan_background_sessions() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Background owner");
        let created = create_agent(&core, "Background Owner".to_string(), agent_node)
            .await
            .unwrap();

        let mut session = ChatSession::new(
            created.id.clone(),
            ModelId::Gpt5.as_serialized_str().to_string(),
        )
        .with_name("Orphan Background Session")
        .with_source(ChatSessionSource::Background, "deleted-task".to_string());
        session.updated_at = time_utils::now_ms();
        core.storage
            .file_sessions
            .write_session(
                &crate::session_log::FileSession::from_chat_session(&session),
                true,
            )
            .unwrap();

        let error = delete_agent(&core, &created.id)
            .await
            .expect_err("background sessions should block deletion");
        assert!(error.to_string().contains("managed sessions exist"));

        let retained_agent = core.storage.agents.get_agent(created.id).unwrap();
        assert!(retained_agent.is_some());
    }

    #[tokio::test]
    async fn test_get_nonexistent_agent_fails() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let result = get_agent(&core, "nonexistent-id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_create_agent_generates_uuid() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        // Verify ID is a valid UUID format
        assert!(!created.id.is_empty());
        assert!(created.id.contains('-')); // UUIDs contain hyphens
        assert_eq!(created.id.len(), 36); // Standard UUID length
    }

    #[tokio::test]
    async fn test_create_agent_sets_timestamps() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let before = time_utils::now_ms();

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        let after = time_utils::now_ms();

        // Verify timestamps are set and within reasonable bounds
        assert!(created.created_at.is_some());
        assert!(created.updated_at.is_some());

        let created_at = created.created_at.unwrap();
        let updated_at = created.updated_at.unwrap();

        assert!(created_at >= before && created_at <= after);
        assert!(updated_at >= before && updated_at <= after);
        assert_eq!(created_at, updated_at); // Should be same on creation
    }

    #[tokio::test]
    async fn test_update_agent_updates_timestamp() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;

        let agent_node = create_test_agent_node("Test prompt");
        let created = create_agent(&core, "Test Agent".to_string(), agent_node)
            .await
            .unwrap();

        // Small delay to ensure timestamp difference
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let updated = update_agent(&core, &created.id, Some("Updated Name".to_string()), None)
            .await
            .unwrap();

        // Updated timestamp should be newer
        assert!(updated.updated_at.unwrap() > created.updated_at.unwrap());
        // Created timestamp should remain the same
        assert_eq!(updated.created_at, created.created_at);
    }

    #[tokio::test]
    async fn test_create_agent_rejects_invalid_temperature() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let mut node = create_test_agent_node("test");
        node.temperature = Some(3.0);

        let err = create_agent(&core, "Invalid Agent".to_string(), node)
            .await
            .expect_err("expected validation error");
        let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
            .expect("validation error payload should be JSON");
        assert_eq!(payload.error_type, "validation_error");
        assert!(payload.errors.iter().any(|e| e.field == "temperature"));
    }

    #[tokio::test]
    async fn test_create_agent_rejects_temperature_on_unsupported_model() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let mut node = create_test_agent_node("test");
        set_test_model(&mut node, ModelId::Gpt5);
        node.temperature = Some(0.5);

        let err = create_agent(&core, "Bad Temp Agent".to_string(), node)
            .await
            .expect_err("expected validation error");
        let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
            .expect("validation error payload should be JSON");
        assert!(
            payload
                .errors
                .iter()
                .any(|e| e.field == "temperature" && e.message.contains("does not support"))
        );
    }

    #[tokio::test]
    async fn test_create_agent_rejects_reasoning_effort_on_non_codex() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let mut node = create_test_agent_node("test");
        // ClaudeSonnet4_5 is not a Codex model
        node.codex_cli_reasoning_effort = Some("high".to_string());

        let err = create_agent(&core, "Bad Effort Agent".to_string(), node)
            .await
            .expect_err("expected validation error");
        let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
            .expect("validation error payload should be JSON");
        assert!(
            payload
                .errors
                .iter()
                .any(|e| e.field == "codex_cli_reasoning_effort"
                    && e.message.contains("only applies to Codex CLI"))
        );
    }

    #[tokio::test]
    async fn test_create_agent_rejects_unknown_tool() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let mut node = create_test_agent_node("test");
        node.tools = Some(vec!["tool_does_not_exist".to_string()]);

        let err = create_agent(&core, "Invalid Tool Agent".to_string(), node)
            .await
            .expect_err("expected validation error");
        let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
            .expect("validation error payload should be JSON");
        assert!(payload.errors.iter().any(|e| e.field == "tools"));
    }

    #[tokio::test]
    async fn test_create_agent_rejects_unknown_skill() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let mut node = create_test_agent_node("test");
        node.skills = Some(vec!["missing-skill".to_string()]);

        let err = create_agent(&core, "Invalid Skill Agent".to_string(), node)
            .await
            .expect_err("expected validation error");
        let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
            .expect("validation error payload should be JSON");
        assert!(payload.errors.iter().any(|e| e.field == "skills"));
    }

    #[tokio::test]
    async fn test_create_agent_rejects_missing_secret_reference() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        let mut node = create_test_agent_node("test");
        node.api_key_config = Some(ApiKeyConfig::Secret("MISSING_SECRET".to_string()));

        let err = create_agent(&core, "Missing Secret Agent".to_string(), node)
            .await
            .expect_err("expected validation error");
        let payload: ValidationErrorResponse = serde_json::from_str(&err.to_string())
            .expect("validation error payload should be JSON");
        assert!(payload.errors.iter().any(|e| e.field == "api_key_config"));
    }

    #[tokio::test]
    async fn test_create_agent_accepts_existing_secret_reference() {
        let (core, _db, _agents, _guard) = create_test_core_isolated().await;
        core.storage
            .secrets
            .set_secret("OPENAI_API_KEY", "secret-value", None)
            .unwrap();

        let mut node = create_test_agent_node("test");
        node.api_key_config = Some(ApiKeyConfig::Secret("OPENAI_API_KEY".to_string()));
        node.tools = Some(vec!["bash".to_string()]);

        let created = create_agent(&core, "Valid Secret Agent".to_string(), node)
            .await
            .expect("expected create to pass");
        assert_eq!(created.name, "Valid Secret Agent");
    }
}
