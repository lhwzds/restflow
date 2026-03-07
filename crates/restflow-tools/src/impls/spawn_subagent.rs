//! spawn_subagent tool - Spawn a sub-agent to work on a task in parallel.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

use super::spawn_subagent_batch::{SpawnSubagentBatchOperation, SpawnSubagentBatchTool};
use crate::impls::spawn_subagent_batch::BatchSubagentSpec;
use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_traits::store::KvStore;
use restflow_traits::{
    DEFAULT_SUBAGENT_TIMEOUT_SECS, InlineSubagentConfig, SpawnRequest, SubagentManager,
};

#[cfg(feature = "ts")]
const TS_EXPORT_TO_WEB_TYPES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/types/generated/"
);

/// Parameters for spawn_subagent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = TS_EXPORT_TO_WEB_TYPES))]
pub struct SpawnSubagentParams {
    /// Operation to perform. Defaults to `spawn`.
    #[serde(default)]
    pub operation: SpawnSubagentBatchOperation,

    /// Agent type to spawn (researcher, coder, reviewer, writer, analyst).
    ///
    /// When omitted, runtime creates a temporary sub-agent from inline config.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,

    /// Task description for single spawn, or transient fallback task for batch spawn.
    ///
    /// Required for single spawn. Optional for team management operations.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task: Option<String>,

    /// Transient per-instance task list for batch or team spawn.
    ///
    /// Tasks are assigned in worker order and are never persisted in saved teams.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tasks: Option<Vec<String>>,

    /// If true, wait for completion. If false (default), run concurrently.
    #[serde(default)]
    pub wait: bool,

    /// Timeout in seconds. If omitted, uses sub-agent manager default timeout.
    pub timeout_secs: Option<u64>,

    /// Optional model override for this spawn (e.g., "minimax/coding-plan").
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,

    /// Optional provider selector paired with model (e.g., "openai-codex").
    #[cfg_attr(feature = "ts", ts(optional))]
    pub provider: Option<String>,

    /// Optional parent execution ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub parent_execution_id: Option<String>,

    /// Optional trace session ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub trace_session_id: Option<String>,

    /// Optional trace scope ID (runtime-injected, internal use).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(default)]
    pub trace_scope_id: Option<String>,

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

    /// Optional list-based worker specs for unified single/multi spawn.
    ///
    /// When provided, this tool enters batch mode and spawns one or more workers.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub workers: Option<Vec<BatchSubagentSpec>>,

    /// Optional team name for batch mode spawn.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub team: Option<String>,

    /// Optionally persist the provided workers as a named team during spawn.
    ///
    /// To save a team without spawning, use `operation = "save_team"` and `team`.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub save_as_team: Option<String>,
}

/// spawn_subagent tool for the shared agent execution engine.
pub struct SpawnSubagentTool {
    manager: Arc<dyn SubagentManager>,
    kv_store: Option<Arc<dyn KvStore>>,
}

impl SpawnSubagentTool {
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

    fn build_inline_config(params: &SpawnSubagentParams) -> Option<InlineSubagentConfig> {
        let config = InlineSubagentConfig {
            name: params.inline_name.clone(),
            system_prompt: params.inline_system_prompt.clone(),
            allowed_tools: params.inline_allowed_tools.clone(),
            max_iterations: params.inline_max_iterations,
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

    fn uses_batch_mode(params: &SpawnSubagentParams) -> bool {
        params.workers.is_some()
            || params.team.is_some()
            || params.save_as_team.is_some()
            || params.tasks.is_some()
    }

    fn routes_to_batch_tool(params: &SpawnSubagentParams) -> bool {
        params.operation != SpawnSubagentBatchOperation::Spawn || Self::uses_batch_mode(params)
    }

    fn normalize_optional_text(value: Option<&str>) -> Option<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn resolve_batch_team(params: &SpawnSubagentParams) -> Result<Option<String>> {
        let team = Self::normalize_optional_text(params.team.as_deref());
        let save_as_team = Self::normalize_optional_text(params.save_as_team.as_deref());

        if params.operation == SpawnSubagentBatchOperation::SaveTeam {
            return match (team, save_as_team) {
                (Some(team_name), Some(alias)) if team_name != alias => Err(ToolError::Tool(
                    "When operation is 'save_team', 'team' and 'save_as_team' must match if both are provided.".to_string(),
                )),
                (Some(team_name), _) => Ok(Some(team_name)),
                (None, Some(alias)) => Ok(Some(alias)),
                (None, None) => Ok(None),
            };
        }

        if params.operation != SpawnSubagentBatchOperation::Spawn && save_as_team.is_some() {
            return Err(ToolError::Tool(
                "'save_as_team' is only supported for 'spawn', or as an alias of 'team' when operation is 'save_team'.".to_string(),
            ));
        }

        Ok(team)
    }
}

#[async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized sub-agent to work on a task in parallel. Use wait_subagents to check completion."
    }

    fn parameters_schema(&self) -> Value {
        let available = self.available_agents();
        let agent_property = if available.is_empty() {
            json!({
                "type": "string",
                "description": "Optional agent ID or name. Omit to create a temporary sub-agent. Call list_subagents to discover available agents."
            })
        } else {
            let enum_values: Vec<String> = available.iter().map(|agent| agent.id.clone()).collect();
            let enum_labels: Vec<String> = available
                .iter()
                .map(|agent| format!("{} ({})", agent.name, agent.id))
                .collect();
            json!({
                "type": "string",
                "enum": enum_values,
                "x-enumNames": enum_labels,
                "description": "Optional agent ID. You can also pass agent name at runtime. Omit to create a temporary sub-agent."
            })
        };

        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["spawn", "save_team", "list_teams", "get_team", "delete_team"],
                    "default": "spawn",
                    "description": "Operation to perform. Use team management operations to save/list/read/delete teams without spawning."
                },
                "agent": agent_property,
                "task": {
                    "type": "string",
                    "description": "Detailed task description for single spawn, or transient fallback task for batch worker specs. Required for single spawn."
                },
                "tasks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Transient per-instance task list for batch/team spawn. Tasks are assigned in worker order and are never persisted in saved teams."
                },
                "wait": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, wait for completion. Applies to spawn only."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": DEFAULT_SUBAGENT_TIMEOUT_SECS,
                    "description": format!(
                        "Timeout in seconds for single spawn or batch spawn (default: {})",
                        DEFAULT_SUBAGENT_TIMEOUT_SECS
                    )
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override for this sub-agent (e.g., 'minimax/coding-plan')"
                },
                "provider": {
                    "type": "string",
                    "description": "Provider selector paired with model override (e.g., 'openai-codex'). Required when model is set."
                },
                "parent_execution_id": {
                    "type": "string",
                    "description": "Optional parent execution ID for context propagation (runtime-injected)"
                },
                "trace_session_id": {
                    "type": "string",
                    "description": "Optional trace session ID for context propagation (runtime-injected)"
                },
                "trace_scope_id": {
                    "type": "string",
                    "description": "Optional trace scope ID for context propagation (runtime-injected)"
                },
                "inline_name": {
                    "type": "string",
                    "description": "Optional temporary sub-agent name when 'agent' is omitted."
                },
                "inline_system_prompt": {
                    "type": "string",
                    "description": "Optional system prompt for temporary sub-agent when 'agent' is omitted."
                },
                "inline_allowed_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional tool allowlist for temporary sub-agent when 'agent' is omitted."
                },
                "inline_max_iterations": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional max iterations for temporary sub-agent when 'agent' is omitted."
                },
                "workers": {
                    "type": "array",
                    "description": "Optional unified list-based batch specs. Use for batch spawn or save_team.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "agent": { "type": "string", "description": "Optional agent ID or name." },
                            "count": { "type": "integer", "minimum": 1, "default": 1, "description": "Number of instances for this worker spec." },
                            "task": { "type": "string", "description": "Optional transient per-worker task override." },
                            "tasks": { "type": "array", "items": { "type": "string" }, "description": "Optional transient per-instance task list for distinct prompts." },
                            "timeout_secs": { "type": "integer", "minimum": 0, "description": "Optional per-worker timeout." },
                            "model": { "type": "string", "description": "Optional model override for this worker." },
                            "provider": { "type": "string", "description": "Optional provider paired with model." },
                            "inline_name": { "type": "string", "description": "Optional temporary sub-agent name." },
                            "inline_system_prompt": { "type": "string", "description": "Optional temporary sub-agent system prompt." },
                            "inline_allowed_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional temporary sub-agent tool allowlist." },
                            "inline_max_iterations": { "type": "integer", "minimum": 1, "description": "Optional temporary sub-agent max iterations." }
                        }
                    }
                },
                "team": {
                    "type": "string",
                    "description": "Team name for save_team/get_team/delete_team, or spawn from a saved team."
                },
                "save_as_team": {
                    "type": "string",
                    "description": "Spawn-only convenience flag to save provided workers as a structural team during spawn. For save-only, use operation='save_team'."
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SpawnSubagentParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        if Self::routes_to_batch_tool(&params) {
            if params.agent.is_some()
                || params.model.is_some()
                || params.provider.is_some()
                || params.inline_name.is_some()
                || params.inline_system_prompt.is_some()
                || params.inline_allowed_tools.is_some()
                || params.inline_max_iterations.is_some()
            {
                return Err(ToolError::Tool(
                    "Batch mode uses 'workers'/'team'; do not combine with single-spawn fields like 'agent', top-level model/provider, or top-level inline settings.".to_string(),
                ));
            }

            let mut batch_tool = SpawnSubagentBatchTool::new(self.manager.clone());
            if let Some(kv_store) = self.kv_store.clone() {
                batch_tool = batch_tool.with_kv_store(kv_store);
            }

            let operation = params.operation.clone();
            let task = Self::normalize_optional_text(params.task.as_deref());
            let tasks = params.tasks.clone();
            let team = Self::resolve_batch_team(&params)?;
            let save_as_team = if operation == SpawnSubagentBatchOperation::Spawn {
                Self::normalize_optional_text(params.save_as_team.as_deref())
            } else {
                None
            };

            return batch_tool
                .execute(json!({
                    "operation": operation,
                    "team": team,
                    "specs": params.workers,
                    "task": task,
                    "tasks": tasks,
                    "wait": params.wait,
                    "timeout_secs": params.timeout_secs,
                    "save_as_team": save_as_team,
                    "parent_execution_id": params.parent_execution_id,
                    "trace_session_id": params.trace_session_id,
                    "trace_scope_id": params.trace_scope_id
                }))
                .await;
        }

        let task = Self::normalize_optional_text(params.task.as_deref()).ok_or_else(|| {
            ToolError::Tool("Single spawn requires non-empty 'task'.".to_string())
        })?;

        let inline_config = Self::build_inline_config(&params);
        if params.agent.is_some() && inline_config.is_some() {
            return Err(ToolError::Tool(
                "Inline temporary-subagent fields cannot be combined with 'agent'.".to_string(),
            ));
        }
        let agent_id = params
            .agent
            .as_deref()
            .map(|requested| self.resolve_agent_id(requested))
            .transpose()?;
        let has_model = params
            .model
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let has_provider = params
            .provider
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        if has_model != has_provider {
            return Err(ToolError::Tool(
                "Model override requires both 'model' and 'provider' fields.".to_string(),
            ));
        }

        let request = SpawnRequest {
            agent_id,
            inline: inline_config,
            task,
            timeout_secs: params.timeout_secs,
            priority: None,
            model: params.model.clone(),
            model_provider: params.provider.clone(),
            parent_execution_id: params.parent_execution_id.clone(),
            trace_session_id: params.trace_session_id.clone(),
            trace_scope_id: params.trace_scope_id.clone(),
        };

        let handle = self.manager.spawn(request)?;

        if params.wait {
            let wait_timeout = params
                .timeout_secs
                .unwrap_or(self.manager.config().subagent_timeout_secs);

            let result = match timeout(
                Duration::from_secs(wait_timeout),
                self.manager.wait(&handle.id),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => return Ok(ToolOutput::error("Sub-agent not found")),
                Err(_) => {
                    return Ok(ToolOutput::success(json!({
                        "agent": handle.agent_name,
                        "status": "timeout",
                        "message": "Timeout waiting for sub-agent"
                    })));
                }
            };

            let output = if result.success {
                json!({
                    "agent": handle.agent_name,
                    "status": "completed",
                    "output": result.output,
                    "duration_ms": result.duration_ms
                })
            } else {
                json!({
                    "agent": handle.agent_name,
                    "status": "failed",
                    "error": result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    "duration_ms": result.duration_ms
                })
            };
            Ok(ToolOutput::success(output))
        } else {
            Ok(ToolOutput::success(json!({
                "task_id": handle.id,
                "agent": handle.agent_name,
                "status": "spawned",
                "message": format!(
                    "Agent '{}' is now working on the task concurrently. Use wait_subagents to check completion.",
                    handle.agent_name
                )
            })))
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
    use crate::Tool;
    use restflow_ai::agent::{
        SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
        SubagentManagerImpl, SubagentTracker,
    };
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use restflow_traits::SubagentManager;
    use restflow_traits::store::KvStore;
    use serde_json::Value;
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
        fn get_entry(&self, key: &str) -> crate::Result<Value> {
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
        ) -> crate::Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.insert(key.to_string(), content.to_string());
            Ok(json!({"success": true, "key": key}))
        }

        fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> crate::Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let deleted = entries.remove(key).is_some();
            Ok(json!({"deleted": deleted, "key": key}))
        }

        fn list_entries(&self, namespace: Option<&str>) -> crate::Result<Value> {
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
                .map(|key| json!({ "key": key }))
                .collect::<Vec<_>>();
            Ok(json!({
                "count": list.len(),
                "entries": list
            }))
        }
    }

    fn make_test_deps(
        agents: Vec<(&str, &str)>,
        mock_steps: Vec<MockStep>,
    ) -> Arc<dyn SubagentManager> {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(MockDefLookup::with_agents(agents));
        let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 5,
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

    #[test]
    fn test_params_deserialization() {
        let json = r#"{"agent": "researcher", "task": "Research topic X"}"#;
        let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.operation, SpawnSubagentBatchOperation::Spawn);
        assert_eq!(params.agent.as_deref(), Some("researcher"));
        assert_eq!(params.task.as_deref(), Some("Research topic X"));
        assert!(params.tasks.is_none());
        assert!(!params.wait);
    }

    #[test]
    fn test_params_with_wait() {
        let json =
            r#"{"agent": "coder", "task": "Write function Y", "wait": true, "timeout_secs": 600}"#;
        let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.agent.as_deref(), Some("coder"));
        assert_eq!(params.task.as_deref(), Some("Write function Y"));
        assert!(params.wait);
        assert_eq!(params.timeout_secs, Some(600));
    }

    #[test]
    fn test_params_with_model_and_provider() {
        let json = r#"{"agent":"coder","task":"Write function","model":"gpt-5.3-codex","provider":"openai-codex"}"#;
        let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.model.as_deref(), Some("gpt-5.3-codex"));
        assert_eq!(params.provider.as_deref(), Some("openai-codex"));
    }

    #[test]
    fn test_params_with_team_operation() {
        let json = r#"{"operation":"save_team","team":"TeamOnly","workers":[{"count":2}]}"#;
        let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.operation, SpawnSubagentBatchOperation::SaveTeam);
        assert_eq!(params.team.as_deref(), Some("TeamOnly"));
        assert!(params.task.is_none());
        assert!(params.tasks.is_none());
    }

    #[tokio::test]
    async fn test_spawn_subagent_background() {
        let deps = make_test_deps(
            vec![("researcher", "Researcher")],
            vec![MockStep::text("research done")],
        );
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "researcher", "task": "Find info", "wait": false}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "spawned");
        assert!(result.result["task_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_spawn_subagent_wait_success() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::text("function written")],
        );
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(
                json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}),
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
        assert!(
            result.result["output"]
                .as_str()
                .unwrap()
                .contains("function written")
        );
    }

    #[tokio::test]
    async fn test_spawn_subagent_wait_failure() {
        let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::error("LLM error")]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(
                json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}),
            )
            .await
            .unwrap();
        assert!(result.success); // ToolOutput is success, but status indicates failure
        assert_eq!(result.result["status"], "failed");
        assert!(result.result["error"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_spawn_subagent_unknown_agent() {
        let deps = make_test_deps(vec![], vec![]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "nonexistent", "task": "Do something"}))
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No callable sub-agents available"));
    }

    #[tokio::test]
    async fn test_spawn_subagent_invalid_params() {
        let deps = make_test_deps(vec![], vec![]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool.execute(json!({"wait": true})).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Single spawn requires non-empty 'task'")
        );
    }

    #[tokio::test]
    async fn test_spawn_subagent_rejects_model_without_provider() {
        let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "coder", "task": "Write code", "model": "gpt-5.3-codex"}))
            .await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires both 'model' and 'provider'")
        );
    }

    #[tokio::test]
    async fn test_spawn_subagent_rejects_provider_without_model() {
        let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "coder", "task": "Write code", "provider": "openai-codex"}))
            .await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires both 'model' and 'provider'")
        );
    }

    #[tokio::test]
    async fn test_spawn_subagent_resolves_by_name() {
        let deps = make_test_deps(
            vec![("agent-123", "Code Planner")],
            vec![MockStep::text("planned")],
        );
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({"agent": "code planner", "task": "plan task", "wait": true}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
    }

    #[tokio::test]
    async fn test_spawn_subagent_without_agent_uses_temporary_mode() {
        let deps = make_test_deps(
            vec![("agent-123", "Code Planner")],
            vec![MockStep::text("planned")],
        );
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({"task": "plan task", "wait": true}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
    }

    #[tokio::test]
    async fn test_spawn_subagent_rejects_inline_fields_with_agent() {
        let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({
                "agent": "coder",
                "task": "Write code",
                "inline_system_prompt": "You are temporary"
            }))
            .await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot be combined")
        );
    }

    #[tokio::test]
    async fn test_spawn_subagent_supports_workers_list_mode() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::text("done-1"), MockStep::text("done-2")],
        );
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({
                "task": "batch task",
                "wait": true,
                "workers": [
                    { "agent": "coder", "count": 2 }
                ]
            }))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
        assert_eq!(result.result["spawned_count"], 2);
    }

    #[tokio::test]
    async fn test_spawn_subagent_rejects_mixed_single_and_workers_mode_fields() {
        let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({
                "task": "batch task",
                "agent": "coder",
                "workers": [
                    { "agent": "coder", "count": 1 }
                ]
            }))
            .await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Batch mode uses 'workers'/'team'")
        );
    }

    #[tokio::test]
    async fn test_spawn_subagent_workers_support_distinct_tasks_list() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::text("done-a"), MockStep::text("done-b")],
        );
        let tool = SpawnSubagentTool::new(deps);
        let result = tool
            .execute(json!({
                "task": "",
                "wait": true,
                "workers": [
                    { "agent": "coder", "tasks": ["task-A", "task-B"] }
                ]
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
        assert_eq!(result.result["spawned_count"], 2);
        let results = result.result["results"]
            .as_array()
            .expect("results should be array");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|entry| entry["status"] == "completed"));
    }

    #[tokio::test]
    async fn test_spawn_subagent_team_supports_runtime_tasks_list() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::text("done-a"), MockStep::text("done-b")],
        );
        let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
        let tool = SpawnSubagentTool::new(deps).with_kv_store(kv_store);

        let saved = tool
            .execute(json!({
                "operation": "save_team",
                "team": "RuntimeTasksTeam",
                "workers": [
                    { "agent": "coder", "count": 2 }
                ]
            }))
            .await
            .unwrap();
        assert!(saved.success);

        let result = tool
            .execute(json!({
                "team": "RuntimeTasksTeam",
                "tasks": ["task-a", "task-b"],
                "wait": true
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.result["status"], "completed");
        assert_eq!(result.result["spawned_count"], 2);
    }

    #[tokio::test]
    async fn test_spawn_subagent_save_team_operation_persists_without_spawning() {
        let deps = make_test_deps(
            vec![("coder", "Coder")],
            vec![MockStep::text("should-not-run")],
        );
        let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
        let tool = SpawnSubagentTool::new(deps.clone()).with_kv_store(kv_store);

        let output = tool
            .execute(json!({
                "operation": "save_team",
                "team": "TeamOnly",
                "workers": [
                    { "agent": "coder", "count": 2 }
                ]
            }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["operation"], "save_team");
        assert_eq!(deps.running_count(), 0);

        let reuse = tool
            .execute(json!({
                "task": "Use saved team",
                "wait": true,
                "team": "TeamOnly"
            }))
            .await
            .unwrap();

        assert!(reuse.success);
        assert_eq!(reuse.result["spawned_count"], 2);
    }

    #[tokio::test]
    async fn test_spawn_subagent_save_team_rejects_prompt_fields() {
        let deps = make_test_deps(vec![("coder", "Coder")], vec![]);
        let kv_store: Arc<dyn KvStore> = Arc::new(MockKvStore::default());
        let tool = SpawnSubagentTool::new(deps).with_kv_store(kv_store);

        let result = tool
            .execute(json!({
                "operation": "save_team",
                "team": "PromptfulTeam",
                "workers": [
                    { "agent": "coder", "task": "Should not persist" }
                ]
            }))
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("stores worker structure only")
        );
    }

    #[test]
    fn test_parameters_schema_uses_dynamic_agent_ids() {
        let deps = make_test_deps(
            vec![("agent-1", "Researcher"), ("agent-2", "Coder")],
            vec![],
        );
        let tool = SpawnSubagentTool::new(deps);
        let schema = tool.parameters_schema();
        let values = schema["properties"]["agent"]["enum"]
            .as_array()
            .expect("agent enum should exist");
        let ids = values
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"agent-1"));
        assert!(ids.contains(&"agent-2"));
        assert_eq!(
            schema["properties"]["timeout_secs"]["default"],
            json!(DEFAULT_SUBAGENT_TIMEOUT_SECS)
        );
    }
}
