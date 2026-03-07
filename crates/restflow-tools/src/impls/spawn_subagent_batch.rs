//! spawn_subagent_batch tool - Batch spawn sub-agents and manage reusable team presets.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

use crate::{Result, Tool, ToolError, ToolOutput};
use restflow_traits::store::KvStore;
use restflow_traits::{InlineSubagentConfig, SpawnRequest, SubagentManager};

#[cfg(feature = "ts")]
const TS_EXPORT_TO_WEB_TYPES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/types/generated/"
);

const SUBAGENT_TEAM_NAMESPACE: &str = "subagent_team";
const SUBAGENT_TEAM_CONTENT_TYPE: &str = "application/json";
const SUBAGENT_TEAM_TYPE_HINT: &str = "subagent_team";
const SUBAGENT_TEAM_VERSION: u32 = 1;

/// Operation for spawn_subagent_batch tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = TS_EXPORT_TO_WEB_TYPES))]
#[serde(rename_all = "snake_case")]
pub enum SpawnSubagentBatchOperation {
    /// Spawn one batch of sub-agents immediately.
    #[default]
    Spawn,
    /// Save a reusable team configuration.
    SaveTeam,
    /// List all saved teams.
    ListTeams,
    /// Get one saved team definition.
    GetTeam,
    /// Delete one saved team definition.
    DeleteTeam,
}

fn default_member_count() -> u32 {
    1
}

/// One batch member specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = TS_EXPORT_TO_WEB_TYPES))]
pub struct BatchSubagentSpec {
    /// Optional agent ID or name.
    ///
    /// If omitted, a temporary sub-agent is created from inline fields or defaults.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,

    /// Number of identical sub-agents to spawn for this spec.
    #[serde(default = "default_member_count")]
    pub count: u32,

    /// Optional transient per-spec task override.
    ///
    /// If omitted, top-level `task` is used. This field is never persisted in saved teams.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Optional transient per-instance task list.
    ///
    /// When provided, each spawned instance uses the corresponding entry in this list.
    /// This allows one worker spec to fan out with distinct prompts. This field is never
    /// persisted in saved teams.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tasks: Option<Vec<String>>,

    /// Optional per-spec timeout (seconds) passed to sub-agent execution.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_secs: Option<u64>,

    /// Optional model override.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,

    /// Optional provider override paired with model.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub provider: Option<String>,

    /// Optional name for temporary sub-agent creation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_name: Option<String>,

    /// Optional system prompt for temporary sub-agent creation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_system_prompt: Option<String>,

    /// Optional allowlist for temporary sub-agent tools.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_allowed_tools: Option<Vec<String>>,

    /// Optional max iterations override for temporary sub-agent creation.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub inline_max_iterations: Option<u32>,
}

/// Parameters for spawn_subagent_batch tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = TS_EXPORT_TO_WEB_TYPES))]
pub struct SpawnSubagentBatchParams {
    /// Operation to perform.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,

    /// Team name for `save_team`, `get_team`, `delete_team`, or `spawn` from saved team.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub team: Option<String>,

    /// Batch member specs.
    ///
    /// For `spawn`, either `specs` or `team` must be provided.
    /// For `save_team`, `specs` is required.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub specs: Option<Vec<BatchSubagentSpec>>,

    /// Default transient task for all specs that do not set per-spec `task`.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Transient per-instance task list for this spawn.
    ///
    /// When provided, tasks are assigned across all instances in spec order and are not
    /// persisted in saved teams.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tasks: Option<Vec<String>>,

    /// If true, wait for all spawned tasks to complete.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds for wait and as fallback spawn timeout.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_secs: Option<u64>,

    /// Optionally persist the provided specs as a named team during `spawn`.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub save_as_team: Option<String>,

    /// Optional parent execution ID for context propagation (runtime-injected).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parent_execution_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StoredSubagentTeam {
    version: u32,
    name: String,
    specs: Vec<StoredBatchSubagentSpec>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
struct StoredBatchSubagentSpec {
    agent: Option<String>,
    count: u32,
    timeout_secs: Option<u64>,
    model: Option<String>,
    provider: Option<String>,
    inline_name: Option<String>,
    inline_system_prompt: Option<String>,
    inline_allowed_tools: Option<Vec<String>>,
    inline_max_iterations: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoredSubagentTeamEnvelope {
    version: u32,
    name: String,
    specs: Vec<Value>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone)]
struct SpawnedTask {
    task_id: String,
    agent_name: String,
    spec_index: usize,
    instance_index: u32,
}

/// spawn_subagent_batch tool for shared agent execution engine.
pub struct SpawnSubagentBatchTool {
    manager: Arc<dyn SubagentManager>,
    kv_store: Option<Arc<dyn KvStore>>,
}

impl SpawnSubagentBatchTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self {
            manager,
            kv_store: None,
        }
    }

    pub fn with_kv_store(mut self, kv_store: Arc<dyn KvStore>) -> Self {
        self.kv_store = Some(kv_store);
        self
    }

    fn available_agents(&self) -> Vec<restflow_traits::subagent::SubagentDefSummary> {
        self.manager.list_callable()
    }

    fn resolve_agent_id(&self, requested: &str) -> Result<String> {
        let query = requested.trim();
        if query.is_empty() {
            return Err(ToolError::Tool("Agent name must not be empty".to_string()));
        }

        let available = self.available_agents();
        if available.is_empty() {
            return Err(ToolError::Tool(
                "No callable sub-agents available. Create an agent first.".to_string(),
            ));
        }

        if let Some(found) = available.iter().find(|agent| agent.id == query) {
            return Ok(found.id.clone());
        }
        if let Some(found) = available
            .iter()
            .find(|agent| agent.id.eq_ignore_ascii_case(query))
        {
            return Ok(found.id.clone());
        }

        let exact_name_matches: Vec<_> = available
            .iter()
            .filter(|agent| agent.name.eq_ignore_ascii_case(query))
            .collect();
        if exact_name_matches.len() == 1 {
            return Ok(exact_name_matches[0].id.clone());
        }
        if exact_name_matches.len() > 1 {
            let ids = exact_name_matches
                .iter()
                .map(|agent| agent.id.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ToolError::Tool(format!(
                "Ambiguous agent name '{}'. Matching IDs: {}",
                query, ids
            )));
        }

        let normalized_query = normalize_identifier(query);
        let normalized_matches: Vec<_> = available
            .iter()
            .filter(|agent| {
                normalize_identifier(&agent.id) == normalized_query
                    || normalize_identifier(&agent.name) == normalized_query
            })
            .collect();
        if normalized_matches.len() == 1 {
            return Ok(normalized_matches[0].id.clone());
        }
        if normalized_matches.len() > 1 {
            let ids = normalized_matches
                .iter()
                .map(|agent| agent.id.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ToolError::Tool(format!(
                "Ambiguous agent identifier '{}'. Matching IDs: {}",
                query, ids
            )));
        }

        let suggestions = available
            .iter()
            .take(8)
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .collect::<Vec<_>>()
            .join(", ");
        Err(ToolError::Tool(format!(
            "Unknown agent '{}'. Available agents: {}",
            query, suggestions
        )))
    }

    fn build_inline_config(spec: &BatchSubagentSpec) -> Option<InlineSubagentConfig> {
        let config = InlineSubagentConfig {
            name: spec.inline_name.clone(),
            system_prompt: spec.inline_system_prompt.clone(),
            allowed_tools: spec.inline_allowed_tools.clone(),
            max_iterations: spec.inline_max_iterations,
        };
        if config.name.is_none()
            && config.system_prompt.is_none()
            && config.allowed_tools.is_none()
            && config.max_iterations.is_none()
        {
            None
        } else {
            Some(config)
        }
    }

    fn validate_model_provider(model: &Option<String>, provider: &Option<String>) -> Result<()> {
        let has_model = model
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let has_provider = provider
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if has_model != has_provider {
            return Err(ToolError::Tool(
                "Model override requires both 'model' and 'provider' fields.".to_string(),
            ));
        }
        Ok(())
    }

    fn team_store(&self) -> Result<Arc<dyn KvStore>> {
        self.kv_store.clone().ok_or_else(|| {
            ToolError::Tool(
                "Team storage is unavailable in this runtime. Provide specs directly.".to_string(),
            )
        })
    }

    fn validate_team_name(name: &str) -> Result<String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ToolError::Tool("Team name must not be empty".to_string()));
        }
        if trimmed.contains(':') {
            return Err(ToolError::Tool(
                "Team name must not contain ':'".to_string(),
            ));
        }
        Ok(trimmed.to_string())
    }

    fn team_key(name: &str) -> Result<String> {
        let normalized = Self::validate_team_name(name)?;
        Ok(format!("{SUBAGENT_TEAM_NAMESPACE}:{normalized}"))
    }

    fn total_instances(specs: &[BatchSubagentSpec]) -> Result<usize> {
        let mut total: usize = 0;
        for (spec_index, spec) in specs.iter().enumerate() {
            if spec.task.is_some() && spec.tasks.is_some() {
                return Err(ToolError::Tool(format!(
                    "Spec index {} cannot set both 'task' and 'tasks'.",
                    spec_index
                )));
            }

            if let Some(tasks) = &spec.tasks {
                if tasks.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} has empty 'tasks'.",
                        spec_index
                    )));
                }

                for (task_index, task) in tasks.iter().enumerate() {
                    if task.trim().is_empty() {
                        return Err(ToolError::Tool(format!(
                            "Spec index {} has empty task at tasks[{}].",
                            spec_index, task_index
                        )));
                    }
                }

                if spec.count != 1 && spec.count as usize != tasks.len() {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} has count={} but tasks.len()={}. Set count to 1 (default) or match tasks length.",
                        spec_index,
                        spec.count,
                        tasks.len()
                    )));
                }

                total = total.saturating_add(tasks.len());
                continue;
            }

            if spec.count == 0 {
                return Err(ToolError::Tool("Each spec count must be >= 1.".to_string()));
            }
            total = total.saturating_add(spec.count as usize);
        }
        if total == 0 {
            return Err(ToolError::Tool("No sub-agents requested.".to_string()));
        }
        Ok(total)
    }

    fn structural_count(spec: &BatchSubagentSpec, spec_index: usize) -> Result<u32> {
        if let Some(tasks) = &spec.tasks {
            return u32::try_from(tasks.len()).map_err(|_| {
                ToolError::Tool(format!(
                    "Spec index {} has too many tasks to store as a team member count.",
                    spec_index
                ))
            });
        }
        Ok(spec.count)
    }

    fn stored_spec_from_batch(
        spec: &BatchSubagentSpec,
        spec_index: usize,
    ) -> Result<StoredBatchSubagentSpec> {
        Ok(StoredBatchSubagentSpec {
            agent: spec.agent.clone(),
            count: Self::structural_count(spec, spec_index)?,
            timeout_secs: spec.timeout_secs,
            model: spec.model.clone(),
            provider: spec.provider.clone(),
            inline_name: spec.inline_name.clone(),
            inline_system_prompt: spec.inline_system_prompt.clone(),
            inline_allowed_tools: spec.inline_allowed_tools.clone(),
            inline_max_iterations: spec.inline_max_iterations,
        })
    }

    fn batch_spec_from_stored_value(value: Value, spec_index: usize) -> Result<BatchSubagentSpec> {
        let spec: BatchSubagentSpec = serde_json::from_value(value).map_err(|err| {
            ToolError::Tool(format!(
                "Stored team has invalid spec at index {}: {}",
                spec_index, err
            ))
        })?;
        let count = Self::structural_count(&spec, spec_index)?;
        Ok(BatchSubagentSpec {
            agent: spec.agent,
            count,
            task: None,
            tasks: None,
            timeout_secs: spec.timeout_secs,
            model: spec.model,
            provider: spec.provider,
            inline_name: spec.inline_name,
            inline_system_prompt: spec.inline_system_prompt,
            inline_allowed_tools: spec.inline_allowed_tools,
            inline_max_iterations: spec.inline_max_iterations,
        })
    }

    fn validate_save_team_request(
        task: Option<&str>,
        tasks: Option<&[String]>,
        specs: &[BatchSubagentSpec],
    ) -> Result<()> {
        if task.is_some() || tasks.is_some() {
            return Err(ToolError::Tool(
                "save_team stores worker structure only. Remove top-level 'task'/'tasks' and pass prompts during spawn.".to_string(),
            ));
        }
        for (spec_index, spec) in specs.iter().enumerate() {
            if spec.task.is_some() || spec.tasks.is_some() {
                return Err(ToolError::Tool(format!(
                    "save_team stores worker structure only. Remove 'task'/'tasks' from spec index {} and pass prompts during spawn.",
                    spec_index
                )));
            }
        }
        Ok(())
    }

    fn resolve_instance_tasks(
        spec: &BatchSubagentSpec,
        fallback_task: Option<&str>,
        spec_index: usize,
    ) -> Result<Vec<String>> {
        if spec.task.is_some() && spec.tasks.is_some() {
            return Err(ToolError::Tool(format!(
                "Spec index {} cannot set both 'task' and 'tasks'.",
                spec_index
            )));
        }

        if let Some(tasks) = &spec.tasks {
            if tasks.is_empty() {
                return Err(ToolError::Tool(format!(
                    "Spec index {} has empty 'tasks'.",
                    spec_index
                )));
            }

            let mut resolved = Vec::with_capacity(tasks.len());
            for (task_index, task) in tasks.iter().enumerate() {
                let trimmed = task.trim();
                if trimmed.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} has empty task at tasks[{}].",
                        spec_index, task_index
                    )));
                }
                resolved.push(trimmed.to_string());
            }
            return Ok(resolved);
        }

        let task = spec
            .task
            .as_deref()
            .or(fallback_task)
            .ok_or_else(|| {
                ToolError::Tool(format!(
                    "Missing task for spec index {}. Provide top-level 'task', top-level 'tasks', per-spec 'task', or per-spec 'tasks'.",
                    spec_index
                ))
            })?;
        let trimmed = task.trim();
        if trimmed.is_empty() {
            return Err(ToolError::Tool(format!(
                "Task for spec index {} must not be empty.",
                spec_index
            )));
        }

        Ok((0..spec.count).map(|_| trimmed.to_string()).collect())
    }

    fn resolve_batch_tasks(
        specs: &[BatchSubagentSpec],
        fallback_task: Option<&str>,
        fallback_tasks: Option<&[String]>,
    ) -> Result<Vec<Vec<String>>> {
        if fallback_task.is_some() && fallback_tasks.is_some() {
            return Err(ToolError::Tool(
                "Use either top-level 'task' or top-level 'tasks', not both.".to_string(),
            ));
        }

        if let Some(tasks) = fallback_tasks {
            if tasks.is_empty() {
                return Err(ToolError::Tool(
                    "Top-level 'tasks' must not be empty.".to_string(),
                ));
            }

            for (spec_index, spec) in specs.iter().enumerate() {
                if spec.task.is_some() || spec.tasks.is_some() {
                    return Err(ToolError::Tool(format!(
                        "Top-level 'tasks' cannot be combined with per-spec 'task' or 'tasks' (spec index {}).",
                        spec_index
                    )));
                }
            }

            let mut normalized = Vec::with_capacity(tasks.len());
            for (task_index, task) in tasks.iter().enumerate() {
                let trimmed = task.trim();
                if trimmed.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Top-level 'tasks' has empty task at index {}.",
                        task_index
                    )));
                }
                normalized.push(trimmed.to_string());
            }

            let expected = Self::total_instances(specs)?;
            if normalized.len() != expected {
                return Err(ToolError::Tool(format!(
                    "Top-level 'tasks' length {} does not match total requested instances {}.",
                    normalized.len(),
                    expected
                )));
            }

            let mut offset = 0usize;
            let mut resolved = Vec::with_capacity(specs.len());
            for (spec_index, spec) in specs.iter().enumerate() {
                let count =
                    usize::try_from(Self::structural_count(spec, spec_index)?).map_err(|_| {
                        ToolError::Tool(format!(
                            "Spec index {} count exceeds supported runtime size.",
                            spec_index
                        ))
                    })?;
                let end = offset + count;
                resolved.push(normalized[offset..end].to_vec());
                offset = end;
            }

            return Ok(resolved);
        }

        specs
            .iter()
            .enumerate()
            .map(|(spec_index, spec)| Self::resolve_instance_tasks(spec, fallback_task, spec_index))
            .collect()
    }

    fn load_team_specs(&self, team_name: &str) -> Result<Vec<BatchSubagentSpec>> {
        let key = Self::team_key(team_name)?;
        let store = self.team_store()?;
        let payload = store.get_entry(&key)?;
        if !payload
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(ToolError::Tool(format!(
                "Team '{}' was not found.",
                team_name
            )));
        }
        let raw = payload
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Tool("Stored team payload is invalid.".to_string()))?;
        let team: StoredSubagentTeamEnvelope = serde_json::from_str(raw).map_err(|err| {
            ToolError::Tool(format!(
                "Stored team '{}' has invalid JSON payload: {}",
                team_name, err
            ))
        })?;
        if team.specs.is_empty() {
            return Err(ToolError::Tool(format!(
                "Team '{}' has no member specs.",
                team_name
            )));
        }
        team.specs
            .into_iter()
            .enumerate()
            .map(|(spec_index, value)| Self::batch_spec_from_stored_value(value, spec_index))
            .collect()
    }

    fn save_team_specs(&self, team_name: &str, specs: &[BatchSubagentSpec]) -> Result<Value> {
        if specs.is_empty() {
            return Err(ToolError::Tool(
                "Cannot save team with empty specs.".to_string(),
            ));
        }
        let key = Self::team_key(team_name)?;
        let store = self.team_store()?;
        let now = chrono::Utc::now().timestamp_millis();

        let existing = store.get_entry(&key)?;
        let created_at = existing
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            .then(|| {
                existing
                    .get("value")
                    .and_then(Value::as_str)
                    .and_then(|raw| serde_json::from_str::<StoredSubagentTeamEnvelope>(raw).ok())
                    .map(|team| team.created_at)
            })
            .flatten()
            .unwrap_or(now);

        let normalized_team_name = Self::validate_team_name(team_name)?;
        let stored_specs = specs
            .iter()
            .enumerate()
            .map(|(spec_index, spec)| Self::stored_spec_from_batch(spec, spec_index))
            .collect::<Result<Vec<_>>>()?;
        let document = StoredSubagentTeam {
            version: SUBAGENT_TEAM_VERSION,
            name: normalized_team_name.clone(),
            specs: stored_specs,
            created_at,
            updated_at: now,
        };
        let serialized = serde_json::to_string(&document)
            .map_err(|err| ToolError::Tool(format!("Failed to serialize team: {}", err)))?;

        let persist_result = store.set_entry(
            &key,
            &serialized,
            Some("private"),
            Some(SUBAGENT_TEAM_CONTENT_TYPE),
            Some(SUBAGENT_TEAM_TYPE_HINT),
            Some(vec!["subagent".to_string(), "team".to_string()]),
            None,
        )?;
        let total = Self::total_instances(specs)?;

        Ok(json!({
            "saved": true,
            "team": normalized_team_name,
            "member_groups": specs.len(),
            "total_instances": total,
            "storage": persist_result
        }))
    }

    fn list_teams(&self) -> Result<ToolOutput> {
        let store = self.team_store()?;
        let payload = store.list_entries(Some(SUBAGENT_TEAM_NAMESPACE))?;
        let entries = payload
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let prefix = format!("{SUBAGENT_TEAM_NAMESPACE}:");
        let teams = entries
            .iter()
            .filter_map(|entry| {
                let key = entry.get("key")?.as_str()?;
                let team = key.strip_prefix(&prefix)?.to_string();
                Some(json!({
                    "team": team,
                    "updated_at": entry.get("updated_at").cloned().unwrap_or(Value::Null),
                    "tags": entry.get("tags").cloned().unwrap_or(Value::Null),
                }))
            })
            .collect::<Vec<_>>();

        Ok(ToolOutput::success(json!({
            "operation": "list_teams",
            "count": teams.len(),
            "teams": teams
        })))
    }

    fn get_team(&self, team_name: &str) -> Result<ToolOutput> {
        let key = Self::team_key(team_name)?;
        let store = self.team_store()?;
        let payload = store.get_entry(&key)?;
        if !payload
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(ToolOutput::error(format!(
                "Team '{}' was not found.",
                team_name
            )));
        }
        let raw = payload
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Tool("Stored team payload is invalid.".to_string()))?;
        let team: StoredSubagentTeamEnvelope = serde_json::from_str(raw)
            .map_err(|err| ToolError::Tool(format!("Failed to parse stored team: {}", err)))?;
        let specs = team
            .specs
            .into_iter()
            .enumerate()
            .map(|(spec_index, value)| Self::batch_spec_from_stored_value(value, spec_index))
            .collect::<Result<Vec<_>>>()?;
        Ok(ToolOutput::success(json!({
            "operation": "get_team",
            "team": team.name,
            "version": team.version,
            "created_at": team.created_at,
            "updated_at": team.updated_at,
            "member_groups": specs.len(),
            "total_instances": Self::total_instances(&specs)?,
            "specs": specs
        })))
    }

    fn delete_team(&self, team_name: &str) -> Result<ToolOutput> {
        let key = Self::team_key(team_name)?;
        let store = self.team_store()?;
        let deleted = store.delete_entry(&key, None)?;
        Ok(ToolOutput::success(json!({
            "operation": "delete_team",
            "team": Self::validate_team_name(team_name)?,
            "result": deleted
        })))
    }

    fn specs_for_spawn(&self, params: &SpawnSubagentBatchParams) -> Result<Vec<BatchSubagentSpec>> {
        if params.team.is_some() && params.specs.is_some() {
            return Err(ToolError::Tool(
                "Use either 'team' or 'specs' for spawn, not both.".to_string(),
            ));
        }

        let specs = if let Some(team_name) = params.team.as_deref() {
            self.load_team_specs(team_name)?
        } else {
            params.specs.clone().ok_or_else(|| {
                ToolError::Tool("Spawn requires either 'team' or 'specs'.".to_string())
            })?
        };

        if specs.is_empty() {
            return Err(ToolError::Tool("Specs must not be empty.".to_string()));
        }

        for spec in &specs {
            if spec.agent.is_some() && Self::build_inline_config(spec).is_some() {
                return Err(ToolError::Tool(
                    "Inline temporary-subagent fields cannot be combined with 'agent'.".to_string(),
                ));
            }
            if spec.task.is_some() && spec.tasks.is_some() {
                return Err(ToolError::Tool(
                    "Each spec can use either 'task' or 'tasks', not both.".to_string(),
                ));
            }
            Self::validate_model_provider(&spec.model, &spec.provider)?;
        }

        Ok(specs)
    }

    async fn wait_result(
        &self,
        task_id: &str,
        timeout_secs: u64,
    ) -> Option<restflow_traits::SubagentResult> {
        if timeout_secs == 0 {
            return self.manager.wait(task_id).await;
        }
        timeout(
            Duration::from_secs(timeout_secs),
            self.manager.wait(task_id),
        )
        .await
        .unwrap_or_default()
    }

    async fn spawn_batch(&self, params: SpawnSubagentBatchParams) -> Result<ToolOutput> {
        let specs = self.specs_for_spawn(&params)?;
        let total_requested = Self::total_instances(&specs)?;
        let max_parallel = self.manager.config().max_parallel_agents;
        let running_now = self.manager.running_count();
        let available_slots = max_parallel.saturating_sub(running_now);
        if total_requested > available_slots {
            return Err(ToolError::Tool(format!(
                "Requested {} sub-agents, but only {} slots are available (running: {}, max_parallel: {}).",
                total_requested, available_slots, running_now, max_parallel
            )));
        }

        if let Some(team_name) = params.save_as_team.as_deref() {
            self.save_team_specs(team_name, &specs)?;
        }

        let resolved_tasks =
            Self::resolve_batch_tasks(&specs, params.task.as_deref(), params.tasks.as_deref())?;

        let mut spawned = Vec::with_capacity(total_requested);
        for (spec_index, (spec, instance_tasks)) in
            specs.iter().zip(resolved_tasks.into_iter()).enumerate()
        {
            let inline = Self::build_inline_config(spec);
            let agent_id = spec
                .agent
                .as_deref()
                .map(|requested| self.resolve_agent_id(requested))
                .transpose()?;

            for (instance_index, task) in instance_tasks.into_iter().enumerate() {
                if instance_index > u32::MAX as usize {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} has too many instances to index as u32.",
                        spec_index
                    )));
                }
                let instance_index = instance_index as u32;

                let request = SpawnRequest {
                    agent_id: agent_id.clone(),
                    inline: inline.clone(),
                    task,
                    timeout_secs: spec.timeout_secs.or(params.timeout_secs),
                    priority: None,
                    model: spec.model.clone(),
                    model_provider: spec.provider.clone(),
                    parent_execution_id: params.parent_execution_id.clone(),
                };
                let handle = self.manager.spawn(request)?;
                spawned.push(SpawnedTask {
                    task_id: handle.id,
                    agent_name: handle.agent_name,
                    spec_index,
                    instance_index,
                });
            }
        }

        if !params.wait {
            let tasks = spawned
                .iter()
                .map(|task| {
                    json!({
                        "task_id": task.task_id,
                        "agent": task.agent_name,
                        "spec_index": task.spec_index,
                        "instance_index": task.instance_index
                    })
                })
                .collect::<Vec<_>>();
            return Ok(ToolOutput::success(json!({
                "operation": "spawn",
                "status": "spawned",
                "spawned_count": spawned.len(),
                "running_before": running_now,
                "max_parallel": max_parallel,
                "team": params.team,
                "saved_team": params.save_as_team,
                "task_ids": spawned.iter().map(|task| task.task_id.clone()).collect::<Vec<_>>(),
                "tasks": tasks
            })));
        }

        let wait_timeout = params
            .timeout_secs
            .unwrap_or(self.manager.config().subagent_timeout_secs);
        let mut results = Vec::with_capacity(spawned.len());
        for task in &spawned {
            let wait_result = self.wait_result(&task.task_id, wait_timeout).await;
            match wait_result {
                Some(result) if result.success => results.push(json!({
                    "task_id": task.task_id,
                    "agent": task.agent_name,
                    "spec_index": task.spec_index,
                    "instance_index": task.instance_index,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms
                })),
                Some(result) => results.push(json!({
                    "task_id": task.task_id,
                    "agent": task.agent_name,
                    "spec_index": task.spec_index,
                    "instance_index": task.instance_index,
                    "status": "failed",
                    "error": result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.duration_ms
                })),
                None => results.push(json!({
                    "task_id": task.task_id,
                    "agent": task.agent_name,
                    "spec_index": task.spec_index,
                    "instance_index": task.instance_index,
                    "status": "timeout"
                })),
            }
        }

        Ok(ToolOutput::success(json!({
            "operation": "spawn",
            "status": "completed",
            "spawned_count": spawned.len(),
            "team": params.team,
            "saved_team": params.save_as_team,
            "results": results
        })))
    }
}

#[async_trait]
impl Tool for SpawnSubagentBatchTool {
    fn name(&self) -> &str {
        "spawn_subagent_batch"
    }

    fn description(&self) -> &str {
        "Batch spawn sub-agents with model/count specs, and optionally save/reuse named team presets."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["spawn", "save_team", "list_teams", "get_team", "delete_team"],
                    "default": "spawn",
                    "description": "Operation to perform."
                },
                "team": {
                    "type": "string",
                    "description": "Team name for save_team/get_team/delete_team, or spawn from saved team."
                },
                "specs": {
                    "type": "array",
                    "description": "Batch member specs. Required for save_team, optional for spawn when team is provided.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "agent": {
                                "type": "string",
                                "description": "Optional agent ID or name. Omit for temporary sub-agent."
                            },
                            "count": {
                                "type": "integer",
                                "minimum": 1,
                                "default": 1,
                                "description": "How many sub-agents to spawn for this spec."
                            },
                            "task": {
                                "type": "string",
                                "description": "Optional per-spec task override."
                            },
                            "tasks": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional per-instance task list. When set, each spawned instance uses one prompt from this list."
                            },
                            "timeout_secs": {
                                "type": "integer",
                                "minimum": 0,
                                "description": "Optional per-spec timeout in seconds."
                            },
                            "model": {
                                "type": "string",
                                "description": "Optional model override."
                            },
                            "provider": {
                                "type": "string",
                                "description": "Optional provider paired with model."
                            },
                            "inline_name": {
                                "type": "string",
                                "description": "Optional temporary sub-agent name."
                            },
                            "inline_system_prompt": {
                                "type": "string",
                                "description": "Optional temporary sub-agent system prompt."
                            },
                            "inline_allowed_tools": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional temporary sub-agent tool allowlist."
                            },
                            "inline_max_iterations": {
                                "type": "integer",
                                "minimum": 1,
                                "description": "Optional temporary sub-agent max iterations."
                            }
                        }
                    }
                },
                "task": {
                    "type": "string",
                    "description": "Transient default task for specs that do not define per-spec 'task' or 'tasks'. Saved teams never persist this field."
                },
                "tasks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Transient per-instance task list for this spawn. Tasks are assigned in spec order and are never persisted in saved teams."
                },
                "wait": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, wait for all spawned tasks."
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Wait timeout and fallback sub-agent timeout (seconds). Use 0 for no wait timeout."
                },
                "save_as_team": {
                    "type": "string",
                    "description": "Optionally save provided specs as a structural team during spawn. Prompt fields are not persisted."
                },
                "parent_execution_id": {
                    "type": "string",
                    "description": "Optional parent execution ID for context propagation (runtime-injected)."
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SpawnSubagentBatchParams = serde_json::from_value(input)
            .map_err(|err| ToolError::Tool(format!("Invalid parameters: {}", err)))?;

        match params.operation {
            SpawnSubagentBatchOperation::Spawn => self.spawn_batch(params).await,
            SpawnSubagentBatchOperation::SaveTeam => {
                let team_name = params
                    .team
                    .as_deref()
                    .ok_or_else(|| ToolError::Tool("save_team requires 'team'.".to_string()))?;
                let specs = params.specs.ok_or_else(|| {
                    ToolError::Tool("save_team requires non-empty 'specs'.".to_string())
                })?;
                Self::validate_save_team_request(
                    params.task.as_deref(),
                    params.tasks.as_deref(),
                    &specs,
                )?;
                let _ = Self::total_instances(&specs)?;
                for spec in &specs {
                    if spec.task.is_some() && spec.tasks.is_some() {
                        return Err(ToolError::Tool(
                            "Each spec can use either 'task' or 'tasks', not both.".to_string(),
                        ));
                    }
                    Self::validate_model_provider(&spec.model, &spec.provider)?;
                }
                let payload = self.save_team_specs(team_name, &specs)?;
                Ok(ToolOutput::success(json!({
                    "operation": "save_team",
                    "result": payload
                })))
            }
            SpawnSubagentBatchOperation::ListTeams => self.list_teams(),
            SpawnSubagentBatchOperation::GetTeam => {
                let team_name = params
                    .team
                    .as_deref()
                    .ok_or_else(|| ToolError::Tool("get_team requires 'team'.".to_string()))?;
                self.get_team(team_name)
            }
            SpawnSubagentBatchOperation::DeleteTeam => {
                let team_name = params
                    .team
                    .as_deref()
                    .ok_or_else(|| ToolError::Tool("delete_team requires 'team'.".to_string()))?;
                self.delete_team(team_name)
            }
        }
    }
}

fn normalize_identifier(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_dash = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }
        if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }
    normalized.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::agent::{
        SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
        SubagentManagerImpl, SubagentTracker,
    };
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use restflow_traits::SubagentManager;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    struct MockDefLookup {
        defs: HashMap<String, SubagentDefSnapshot>,
        summaries: Vec<SubagentDefSummary>,
    }

    impl MockDefLookup {
        fn with_agents(agents: Vec<(&str, &str)>) -> Self {
            let mut defs = HashMap::new();
            let mut summaries = Vec::new();
            for (id, name) in agents {
                defs.insert(
                    id.to_string(),
                    SubagentDefSnapshot {
                        name: name.to_string(),
                        system_prompt: format!("You are a {} agent.", name),
                        allowed_tools: vec![],
                        max_iterations: Some(1),
                        default_model: None,
                    },
                );
                summaries.push(SubagentDefSummary {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: format!("{} agent", name),
                    tags: vec![],
                });
            }
            Self { defs, summaries }
        }
    }

    impl SubagentDefLookup for MockDefLookup {
        fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
            self.defs.get(id).cloned()
        }
        fn list_callable(&self) -> Vec<SubagentDefSummary> {
            self.summaries.clone()
        }
    }

    #[derive(Default)]
    struct MockKvStore {
        entries: Mutex<HashMap<String, String>>,
    }

    impl KvStore for MockKvStore {
        fn get_entry(&self, key: &str) -> Result<Value> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(value) = entries.get(key) {
                Ok(json!({
                    "found": true,
                    "key": key,
                    "value": value
                }))
            } else {
                Ok(json!({
                    "found": false,
                    "key": key
                }))
            }
        }

        fn set_entry(
            &self,
            key: &str,
            content: &str,
            _visibility: Option<&str>,
            _content_type: Option<&str>,
            _type_hint: Option<&str>,
            _tags: Option<Vec<String>>,
            _accessor_id: Option<&str>,
        ) -> Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.insert(key.to_string(), content.to_string());
            Ok(json!({"success": true, "key": key}))
        }

        fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let deleted = entries.remove(key).is_some();
            Ok(json!({"deleted": deleted, "key": key}))
        }

        fn list_entries(&self, namespace: Option<&str>) -> Result<Value> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefix = namespace.map(|value| format!("{value}:"));
            let list = entries
                .keys()
                .filter(|key| {
                    prefix
                        .as_ref()
                        .map(|value| key.starts_with(value))
                        .unwrap_or(true)
                })
                .map(|key| json!({"key": key}))
                .collect::<Vec<_>>();
            Ok(json!({
                "count": list.len(),
                "entries": list
            }))
        }
    }

    fn make_test_manager(
        agents: Vec<(&str, &str)>,
        mock_steps: Vec<MockStep>,
    ) -> Arc<dyn SubagentManager> {
        let (tx, rx) = mpsc::channel(32);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agents(agents));
        let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 20,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };
        Arc::new(SubagentManagerImpl::new(
            tracker,
            definitions,
            llm_client,
            tool_registry,
            config,
        ))
    }

    #[tokio::test]
    async fn test_spawn_batch_waits_for_all_instances() {
        let manager = make_test_manager(
            vec![("coder", "Coder")],
            vec![
                MockStep::text("done-1"),
                MockStep::text("done-2"),
                MockStep::text("done-3"),
            ],
        );
        let tool = SpawnSubagentBatchTool::new(manager);
        let output = tool
            .execute(json!({
                "operation": "spawn",
                "task": "Implement fixes",
                "wait": true,
                "specs": [
                    { "agent": "coder", "count": 3 }
                ]
            }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["status"], "completed");
        assert_eq!(output.result["spawned_count"], 3);
        assert_eq!(output.result["results"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_spawn_batch_rejects_provider_without_model() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentBatchTool::new(manager);
        let result = tool
            .execute(json!({
                "operation": "spawn",
                "task": "Implement fixes",
                "specs": [
                    { "agent": "coder", "count": 1, "provider": "openai-codex" }
                ]
            }))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires both 'model' and 'provider'"));
    }

    #[tokio::test]
    async fn test_spawn_batch_supports_distinct_tasks_list() {
        let manager = make_test_manager(
            vec![("coder", "Coder")],
            vec![
                MockStep::text("done-1"),
                MockStep::text("done-2"),
                MockStep::text("done-3"),
            ],
        );
        let tool = SpawnSubagentBatchTool::new(manager);
        let output = tool
            .execute(json!({
                "operation": "spawn",
                "wait": true,
                "specs": [
                    { "agent": "coder", "tasks": ["task-1", "task-2", "task-3"] }
                ]
            }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["status"], "completed");
        assert_eq!(output.result["spawned_count"], 3);
        let results = output.result["results"]
            .as_array()
            .expect("results should be array");
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|entry| entry["status"] == "completed"));
    }

    #[tokio::test]
    async fn test_spawn_batch_rejects_task_and_tasks_together() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentBatchTool::new(manager);

        let result = tool
            .execute(json!({
                "operation": "spawn",
                "specs": [
                    { "agent": "coder", "task": "single", "tasks": ["task-1"] }
                ]
            }))
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("either 'task' or 'tasks'"));
    }

    #[tokio::test]
    async fn test_spawn_batch_rejects_tasks_count_mismatch() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentBatchTool::new(manager);

        let result = tool
            .execute(json!({
                "operation": "spawn",
                "specs": [
                    { "agent": "coder", "count": 2, "tasks": ["task-1", "task-2", "task-3"] }
                ]
            }))
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Set count to 1 (default) or match tasks length"));
    }

    #[tokio::test]
    async fn test_spawn_batch_rejects_team_and_specs_combined() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
        let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

        let result = tool
            .execute(json!({
                "operation": "spawn",
                "team": "Team1",
                "specs": [
                    { "agent": "coder", "count": 1 }
                ],
                "task": "Implement fixes"
            }))
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("either 'team' or 'specs'"));
    }

    #[tokio::test]
    async fn test_spawn_batch_requires_task_when_spec_has_no_override() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentBatchTool::new(manager);

        let result = tool
            .execute(json!({
                "operation": "spawn",
                "specs": [
                    { "agent": "coder", "count": 1 }
                ]
            }))
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing task for spec index 0"));
    }

    #[tokio::test]
    async fn test_spawn_batch_rejects_when_requested_instances_exceed_slots() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentBatchTool::new(manager);

        let result = tool
            .execute(json!({
                "operation": "spawn",
                "task": "Implement fixes",
                "specs": [
                    { "agent": "coder", "count": 21 }
                ]
            }))
            .await;

        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("Requested 21 sub-agents"));
        assert!(message.contains("max_parallel: 20"));
    }

    #[tokio::test]
    async fn test_spawn_batch_save_as_team_requires_store() {
        let manager = make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentBatchTool::new(manager);

        let result = tool
            .execute(json!({
                "operation": "spawn",
                "task": "Implement fixes",
                "save_as_team": "NoStore",
                "specs": [
                    { "agent": "coder", "count": 1 }
                ]
            }))
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Team storage is unavailable"));
    }

    #[tokio::test]
    async fn test_spawn_batch_save_as_team_persists_team() {
        let manager = make_test_manager(
            vec![("coder", "Coder")],
            vec![MockStep::text("done-1"), MockStep::text("done-2")],
        );
        let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
        let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

        let spawn_output = tool
            .execute(json!({
                "operation": "spawn",
                "wait": true,
                "save_as_team": "SavedTeam",
                "specs": [
                    { "agent": "coder", "tasks": ["task-1", "task-2"] }
                ]
            }))
            .await
            .unwrap();
        assert!(spawn_output.success);
        assert_eq!(spawn_output.result["saved_team"], "SavedTeam");

        let get_output = tool
            .execute(json!({
                "operation": "get_team",
                "team": "SavedTeam"
            }))
            .await
            .unwrap();
        assert!(get_output.success);
        assert_eq!(get_output.result["team"], "SavedTeam");
        assert_eq!(get_output.result["total_instances"], 2);
        let spec = get_output.result["specs"][0].clone();
        assert_eq!(spec["count"], 2);
        assert!(spec.get("task").is_none() || spec["task"].is_null());
        assert!(spec.get("tasks").is_none() || spec["tasks"].is_null());
    }

    #[tokio::test]
    async fn test_team_lifecycle_and_spawn_from_team() {
        let manager = make_test_manager(
            vec![("coder", "Coder")],
            vec![MockStep::text("done-1"), MockStep::text("done-2")],
        );
        let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
        let tool = SpawnSubagentBatchTool::new(manager).with_kv_store(kv_store);

        let save_output = tool
            .execute(json!({
                "operation": "save_team",
                "team": "Team1",
                "specs": [
                    { "agent": "coder", "count": 2 }
                ]
            }))
            .await
            .unwrap();
        assert!(save_output.success);
        assert_eq!(save_output.result["operation"], "save_team");

        let list_output = tool
            .execute(json!({"operation": "list_teams"}))
            .await
            .unwrap();
        assert!(list_output.success);
        let teams = list_output.result["teams"].as_array().unwrap();
        assert!(teams.iter().any(|entry| entry["team"] == "Team1"));

        let get_output = tool
            .execute(json!({"operation": "get_team", "team": "Team1"}))
            .await
            .unwrap();
        assert!(get_output.success);
        assert_eq!(get_output.result["team"], "Team1");
        assert_eq!(get_output.result["total_instances"], 2);

        let spawn_output = tool
            .execute(json!({
                "operation": "spawn",
                "team": "Team1",
                "task": "Run review",
                "wait": true
            }))
            .await
            .unwrap();
        assert!(spawn_output.success);
        assert_eq!(spawn_output.result["spawned_count"], 2);

        let delete_output = tool
            .execute(json!({"operation": "delete_team", "team": "Team1"}))
            .await
            .unwrap();
        assert!(delete_output.success);
        assert_eq!(delete_output.result["operation"], "delete_team");
    }
}
