//! Typed agent storage wrapper.

use crate::models::AgentNode;
use crate::prompt_files;
use crate::time_utils;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

/// Canonical default assistant name created during app initialization.
pub const DEFAULT_ASSISTANT_NAME: &str = "Default Assistant";

/// Stored agent with metadata
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct StoredAgent {
    pub id: String,
    pub name: String,
    pub agent: AgentNode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_file: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AgentFileFrontmatter {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<crate::models::ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_preflight_policy_mode: Option<crate::models::SkillPreflightPolicyMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// Typed agent storage wrapper around process-local agent bytes.
#[derive(Clone)]
pub struct AgentStorage {
    delete_lock: Arc<Mutex<()>>,
}

impl AgentStorage {
    pub fn new(_db: Arc<redb::Database>) -> Result<Self> {
        Ok(Self {
            delete_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn new_namespace(_namespace: usize) -> Result<Self> {
        Ok(Self {
            delete_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn create_agent(&self, name: String, mut agent: AgentNode) -> Result<StoredAgent> {
        normalize_model_fields(&mut agent)?;
        let now = time_utils::now_ms();
        let id = Uuid::new_v4().to_string();

        // Prompt content is file-backed under ~/.restflow/agents/{agent-name}.md, not stored in DB.
        let prompt_override = agent.prompt.take();
        let prompt_file = if agent_file_catalog_enabled() {
            let prompt_path = prompt_files::ensure_agent_prompt_file(
                &id,
                &name,
                None,
                prompt_override.as_deref(),
            )?;
            agent.prompt = read_agent_prompt_body(&prompt_path)?;
            Some(path_file_name(&prompt_path)?)
        } else {
            agent.prompt = prompt_override;
            None
        };

        let stored_agent = StoredAgent {
            id,
            name,
            agent,
            prompt_file,
            created_at: Some(now),
            updated_at: Some(now),
        };

        self.persist_without_prompt(&stored_agent)?;

        Ok(stored_agent)
    }

    pub fn get_agent(&self, id: String) -> Result<Option<StoredAgent>> {
        if let Some(agent) = self.get_file_agent(&id)? {
            return Ok(Some(agent));
        }

        Ok(None)
    }

    pub fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        self.list_file_agents()
    }

    /// Resolve the default chat agent deterministically.
    ///
    /// Resolution order:
    /// 1. Agent named "Default Assistant" (case-insensitive)
    /// 2. The only existing agent (when exactly one exists)
    ///
    /// This intentionally avoids selecting an arbitrary first agent when
    /// multiple agents exist.
    pub fn resolve_default_agent(&self) -> Result<StoredAgent> {
        let agents = self.list_agents()?;

        if agents.is_empty() {
            anyhow::bail!("No agents configured");
        }

        if let Some(agent) = agents
            .iter()
            .find(|agent| agent.name.eq_ignore_ascii_case(DEFAULT_ASSISTANT_NAME))
            .cloned()
        {
            return Ok(agent);
        }

        if agents.len() == 1 {
            return Ok(agents[0].clone());
        }

        anyhow::bail!(
            "Default agent is ambiguous: define an agent named '{}'",
            DEFAULT_ASSISTANT_NAME
        )
    }

    /// Resolve only the ID of the default chat agent.
    pub fn resolve_default_agent_id(&self) -> Result<String> {
        Ok(self.resolve_default_agent()?.id)
    }

    pub fn update_agent(
        &self,
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        let mut existing_agent = self
            .get_agent(id.clone())?
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", id))?;

        if let Some(new_name) = name {
            existing_agent.name = new_name;
        }

        let mut prompt_override: Option<String> = None;
        if let Some(mut new_agent) = agent {
            normalize_model_fields(&mut new_agent)?;
            prompt_override = new_agent.prompt.take();
            existing_agent.agent = new_agent;
        }

        if agent_file_catalog_enabled() {
            prompt_files::ensure_agent_prompt_file(
                &existing_agent.id,
                &existing_agent.name,
                existing_agent.prompt_file.as_deref(),
                prompt_override.as_deref(),
            )
            .and_then(|path| {
                existing_agent.agent.prompt = read_agent_prompt_body(&path)?;
                path_file_name(&path)
            })
            .map(|prompt_file| existing_agent.prompt_file = Some(prompt_file))?;
        } else if let Some(prompt) = prompt_override {
            existing_agent.agent.prompt = Some(prompt);
        }

        let now = time_utils::now_ms();
        existing_agent.updated_at = Some(now);

        self.persist_without_prompt(&existing_agent)?;

        Ok(existing_agent)
    }

    /// Delete an agent atomically to prevent TOCTOU race conditions.
    ///
    /// This operation resolves the agent ID and deletes it within a single
    /// write transaction, ensuring that concurrent delete operations on the
    /// same agent are handled correctly.
    ///
    /// # Errors
    /// Returns an error if the agent is not found or if the ID prefix is ambiguous.
    pub fn delete_agent(&self, id: String) -> Result<()> {
        let _delete_guard = self
            .delete_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent delete lock poisoned"))?;
        if let Some(existing) = self.get_file_agent(&id)? {
            if let Some(prompt_file) = existing.prompt_file.as_deref() {
                let path = prompt_files::ensure_agents_dir()?.join(prompt_file);
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        anyhow::bail!("Agent {} not found", id);
                    }
                    Err(error) => {
                        return Err(anyhow::anyhow!(
                            "Failed to remove agent file {}: {error}",
                            path.display()
                        ));
                    }
                }
            }
            return Ok(());
        }

        anyhow::bail!("Agent {} not found", id)
    }

    pub fn resolve_existing_agent_id(&self, id_or_prefix: &str) -> Result<String> {
        let id = id_or_prefix.trim();
        if id.is_empty() {
            anyhow::bail!("Agent ID is empty");
        }

        if let Some(agent) = self.get_file_agent(id)? {
            return Ok(agent.id);
        }

        match self.resolve_agent_id_candidate(id)? {
            Some(resolved) => Ok(resolved),
            None => anyhow::bail!("Agent {} not found", id),
        }
    }

    pub fn reconcile_prompt_file_names(&self) -> Result<()> {
        let agents = self.list_agents()?;
        for mut agent in agents {
            let prompt_path = prompt_files::ensure_agent_prompt_file(
                &agent.id,
                &agent.name,
                agent.prompt_file.as_deref(),
                None,
            )?;
            let prompt_file = path_file_name(&prompt_path)?;
            if agent.prompt_file.as_deref() != Some(prompt_file.as_str()) {
                agent.prompt_file = Some(prompt_file);
                self.persist_without_prompt(&agent)?;
            }
        }
        Ok(())
    }

    fn persist_without_prompt(&self, stored: &StoredAgent) -> Result<()> {
        if agent_file_catalog_enabled() {
            self.write_agent_file(stored)?;
        }
        Ok(())
    }

    fn get_file_agent(&self, id_or_prefix: &str) -> Result<Option<StoredAgent>> {
        let candidate = id_or_prefix.trim();
        if candidate.is_empty() {
            return Ok(None);
        }
        let agents = self.list_file_agents()?;
        if let Some(agent) = agents.iter().find(|agent| agent.id == candidate).cloned() {
            return Ok(Some(agent));
        }
        let matches = agents
            .into_iter()
            .filter(|agent| agent.id.starts_with(candidate))
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(matches.into_iter().next()),
            _ => {
                let preview = matches
                    .iter()
                    .take(5)
                    .map(|agent| agent.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "Agent ID prefix '{}' is ambiguous ({} matches: {})",
                    candidate,
                    matches.len(),
                    preview
                )
            }
        }
    }

    fn list_file_agents(&self) -> Result<Vec<StoredAgent>> {
        if !agent_file_catalog_enabled() {
            return Ok(Vec::new());
        }
        let agents_dir = prompt_files::ensure_agents_dir()?;
        let mut agents = Vec::new();
        for entry in fs::read_dir(&agents_dir).map_err(|error| {
            anyhow::anyhow!(
                "Failed to read agents directory {}: {error}",
                agents_dir.display()
            )
        })? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            if path.file_name().and_then(|value| value.to_str()) == Some("task.md") {
                continue;
            }
            if let Some(agent) = load_file_agent(&path)? {
                agents.push(agent);
            }
        }
        agents.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(agents)
    }

    fn write_agent_file(&self, stored: &StoredAgent) -> Result<()> {
        let prompt = stored
            .agent
            .prompt
            .clone()
            .or_else(|| prompt_files::load_default_main_agent_prompt().ok())
            .unwrap_or_default();
        let file_name = stored.prompt_file.clone().unwrap_or_else(|| {
            format!(
                "{}.md",
                prompt_files::sanitize_agent_file_stem(&stored.name)
            )
        });
        let path = prompt_files::ensure_agents_dir()?.join(file_name);
        let content = render_agent_file(stored, &prompt)?;
        fs::write(&path, content).map_err(|error| {
            anyhow::anyhow!("Failed to write agent file {}: {error}", path.display())
        })
    }

    fn resolve_agent_id_candidate(&self, id_or_prefix: &str) -> Result<Option<String>> {
        let prefix = id_or_prefix.trim();
        if prefix.is_empty() {
            return Ok(None);
        }

        if agent_file_catalog_enabled() {
            let matches: Vec<String> = self
                .list_file_agents()?
                .into_iter()
                .map(|agent| agent.id)
                .filter(|id| id.starts_with(prefix))
                .collect();
            match matches.len() {
                0 => {}
                1 => return Ok(matches.into_iter().next()),
                _ => {
                    let preview = matches
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    anyhow::bail!(
                        "Agent ID prefix '{}' is ambiguous ({} matches: {})",
                        prefix,
                        matches.len(),
                        preview
                    );
                }
            }
        }

        Ok(None)
    }
}

fn agent_file_catalog_enabled() -> bool {
    #[cfg(test)]
    {
        std::env::var_os(prompt_files::AGENTS_DIR_ENV).is_some()
            || std::env::var_os("RESTFLOW_DIR").is_some()
    }
    #[cfg(not(test))]
    {
        true
    }
}

fn normalize_model_fields(agent: &mut AgentNode) -> Result<()> {
    if let Err(error) = agent.normalize_model_fields() {
        anyhow::bail!(crate::models::encode_validation_error(vec![error]));
    }
    Ok(())
}

fn render_agent_file(stored: &StoredAgent, prompt: &str) -> Result<String> {
    let frontmatter = AgentFileFrontmatter {
        id: stored.id.clone(),
        name: stored.name.clone(),
        model_ref: stored.agent.model_ref,
        tools: stored.agent.tools.clone(),
        skills: stored.agent.skills.clone(),
        skill_variables: stored.agent.skill_variables.clone(),
        skill_preflight_policy_mode: stored.agent.skill_preflight_policy_mode,
        created_at: stored.created_at,
        updated_at: stored.updated_at,
    };
    let yaml = serde_yaml::to_string(&frontmatter)?;
    let yaml = yaml.strip_prefix("---\n").unwrap_or(&yaml);
    Ok(format!("---\n{}---\n\n{}", yaml, prompt.trim_start()))
}

fn read_agent_prompt_body(path: &std::path::Path) -> Result<Option<String>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).map_err(|error| {
                anyhow::anyhow!("Failed to read agent file {}: {error}", path.display())
            });
        }
    };
    Ok(match parse_agent_file(&content)? {
        Some((_, prompt)) => prompt,
        None => Some(content),
    })
}

fn load_file_agent(path: &std::path::Path) -> Result<Option<StoredAgent>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).map_err(|error| {
                anyhow::anyhow!("Failed to read agent file {}: {error}", path.display())
            });
        }
    };
    let Some((frontmatter, prompt)) = parse_agent_file(&content)? else {
        return Ok(None);
    };
    if frontmatter.id.trim().is_empty() || frontmatter.name.trim().is_empty() {
        return Ok(None);
    }
    let modified = path
        .metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64);
    let mut agent = AgentNode::new();
    agent.model_ref = frontmatter.model_ref;
    agent.prompt = prompt;
    agent.tools = frontmatter.tools;
    agent.skills = frontmatter.skills;
    agent.skill_variables = frontmatter.skill_variables;
    agent.skill_preflight_policy_mode = frontmatter.skill_preflight_policy_mode;
    Ok(Some(StoredAgent {
        id: frontmatter.id,
        name: frontmatter.name,
        agent,
        prompt_file: Some(path_file_name(path)?),
        created_at: frontmatter.created_at.or(modified),
        updated_at: frontmatter.updated_at.or(modified),
    }))
}

fn parse_agent_file(content: &str) -> Result<Option<(AgentFileFrontmatter, Option<String>)>> {
    let Some(rest) = content.strip_prefix("---\n") else {
        return Ok(None);
    };
    let Some((frontmatter, body)) = rest.split_once("\n---") else {
        return Ok(None);
    };
    let body = body
        .strip_prefix("\n\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);
    let frontmatter = serde_yaml::from_str::<AgentFileFrontmatter>(frontmatter)?;
    let prompt = if body.trim().is_empty() {
        None
    } else {
        Some(body.to_string())
    };
    Ok(Some((frontmatter, prompt)))
}

fn path_file_name(path: &std::path::Path) -> Result<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("Invalid prompt path: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelId;
    use crate::prompt_files;
    use redb::{Database, ReadableDatabase};
    use tempfile::tempdir;

    const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        prompt_files::agents_dir_env_lock()
    }

    fn create_test_agent_node() -> AgentNode {
        use crate::models::ApiKeyConfig;

        AgentNode {
            model_ref: Some(crate::models::ModelRef::from_model(
                ModelId::ClaudeSonnet4_5,
            )),
            prompt: Some("You are a helpful assistant".to_string()),
            temperature: Some(0.7),
            codex_cli_reasoning_effort: None,
            codex_cli_execution_mode: None,
            api_key_config: Some(ApiKeyConfig::Direct("test_key".to_string())),
            tools: Some(vec!["add".to_string()]),
            skills: None,
            skill_variables: None,
            skill_preflight_policy_mode: None,
            model_routing: None,
        }
    }

    #[test]
    fn test_insert_and_get_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let agent_node = create_test_agent_node();
        let stored = storage
            .create_agent("Test Agent".to_string(), agent_node)
            .unwrap();

        assert!(!stored.id.is_empty());
        assert_eq!(stored.name, "Test Agent");

        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_some());

        let agent = retrieved.unwrap();
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(
            agent
                .agent
                .resolved_model_ref()
                .map(|model_ref| model_ref.model),
            Some(ModelId::ClaudeSonnet4_5)
        );
        assert!(prompts_dir.join("test-agent.md").exists());
        unsafe {
            std::env::remove_var(AGENTS_DIR_ENV);
        }
    }

    #[test]
    fn test_list_agents() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage
            .create_agent("Agent 1".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .create_agent("Agent 2".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .create_agent("Agent 3".to_string(), create_test_agent_node())
            .unwrap();

        let agents = storage.list_agents().unwrap();
        assert_eq!(agents.len(), 3);

        let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
        assert!(names.contains(&"Agent 1".to_string()));
        assert!(names.contains(&"Agent 2".to_string()));
        assert!(names.contains(&"Agent 3".to_string()));
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .create_agent("Original Name".to_string(), create_test_agent_node())
            .unwrap();
        let updated = storage
            .update_agent(stored.id.clone(), Some("Updated Name".to_string()), None)
            .unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(
            updated
                .agent
                .resolved_model_ref()
                .map(|model_ref| model_ref.model),
            Some(ModelId::ClaudeSonnet4_5)
        );

        let mut new_agent_node = create_test_agent_node();
        new_agent_node.temperature = Some(0.9);

        let updated2 = storage
            .update_agent(stored.id.clone(), None, Some(new_agent_node))
            .unwrap();

        assert_eq!(updated2.name, "Updated Name");
        assert_eq!(updated2.agent.temperature, Some(0.9));
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_name_keeps_redb_tables_empty_for_file_backed_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db.clone()).unwrap();

        let stored = storage
            .create_agent("Original Name".to_string(), create_test_agent_node())
            .unwrap();
        let updated = storage
            .update_agent(stored.id.clone(), Some("Updated Name".to_string()), None)
            .unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert!(updated.agent.prompt.is_some());

        let read_txn = db.begin_read().unwrap();
        let tables = read_txn.list_tables().unwrap().collect::<Vec<_>>();
        assert!(
            tables.is_empty(),
            "file-backed agents must not create redb tables"
        );

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_agent_renames_prompt_file_on_name_change() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .create_agent("Original Name".to_string(), create_test_agent_node())
            .unwrap();
        assert!(prompts_dir.join("original-name.md").exists());

        storage
            .update_agent(stored.id.clone(), Some("Renamed Agent".to_string()), None)
            .unwrap();

        assert!(!prompts_dir.join("original-name.md").exists());
        assert!(prompts_dir.join("renamed-agent.md").exists());
        let content = fs::read_to_string(prompts_dir.join("renamed-agent.md")).unwrap();
        let (_, prompt) = parse_agent_file(&content)
            .unwrap()
            .expect("renamed agent file should contain frontmatter");
        assert_eq!(prompt.as_deref(), Some("You are a helpful assistant"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_get_agent_supports_unique_prefix() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .create_agent("Prefix Test".to_string(), create_test_agent_node())
            .unwrap();
        let short = stored.id.chars().take(8).collect::<String>();
        let resolved = storage
            .get_agent(short)
            .unwrap()
            .expect("agent should resolve");
        assert_eq!(resolved.id, stored.id);

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_delete_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let stored = storage
            .create_agent("To Delete".to_string(), create_test_agent_node())
            .unwrap();
        storage.delete_agent(stored.id.clone()).unwrap();

        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_none());

        let deleted_again = storage.delete_agent(stored.id);
        assert!(deleted_again.is_err());
        assert!(deleted_again.unwrap_err().to_string().contains("not found"));
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_get_nonexistent_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let result = storage.get_agent("nonexistent".to_string()).unwrap();
        assert!(result.is_none());
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_update_nonexistent_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let result = storage.update_agent(
            "nonexistent".to_string(),
            Some("New Name".to_string()),
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_resolve_default_agent_prefers_default_assistant() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let first = storage
            .create_agent("Issue Finder Agent".to_string(), create_test_agent_node())
            .unwrap();
        let default_agent = storage
            .create_agent(DEFAULT_ASSISTANT_NAME.to_string(), create_test_agent_node())
            .unwrap();

        let resolved = storage.resolve_default_agent().unwrap();
        assert_eq!(resolved.id, default_agent.id);
        assert_ne!(resolved.id, first.id);

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_resolve_default_agent_uses_only_agent() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let only = storage
            .create_agent("Only Agent".to_string(), create_test_agent_node())
            .unwrap();

        let resolved = storage.resolve_default_agent().unwrap();
        assert_eq!(resolved.id, only.id);
        assert_eq!(storage.resolve_default_agent_id().unwrap(), only.id);

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_resolve_default_agent_errors_when_ambiguous() {
        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage
            .create_agent("Issue Finder Agent".to_string(), create_test_agent_node())
            .unwrap();
        storage
            .create_agent("Feature B".to_string(), create_test_agent_node())
            .unwrap();

        let err = storage.resolve_default_agent().expect_err("should fail");
        assert!(err.to_string().contains("Default agent is ambiguous"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    /// Test concurrent delete_agent operations don't cause race conditions.
    /// Only one thread should succeed in deleting the agent.
    #[test]
    fn test_concurrent_delete_agent_atomic() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let _lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let prompts_dir = temp_dir.path().join("agents");
        unsafe { std::env::set_var(AGENTS_DIR_ENV, &prompts_dir) };
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(AgentStorage::new(db).unwrap());

        let stored = storage
            .create_agent("Race Test".to_string(), create_test_agent_node())
            .unwrap();

        let success_count = Arc::new(AtomicUsize::new(0));
        let num_threads = 10;

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let s = Arc::clone(&storage);
                let id = stored.id.clone();
                let count = Arc::clone(&success_count);
                thread::spawn(move || {
                    if s.delete_agent(id).is_ok() {
                        count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Exactly one delete should have succeeded
        assert_eq!(success_count.load(Ordering::SeqCst), 1);

        // Agent should no longer exist
        let retrieved = storage.get_agent(stored.id.clone()).unwrap();
        assert!(retrieved.is_none());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }
}
