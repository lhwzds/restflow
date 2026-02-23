//! Sub-agent spawning support for tool-based execution.

use super::tracker::{SubagentResult, SubagentTracker};
use crate::runtime::agent::ToolRegistry;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use restflow_ai::LlmClient;
use restflow_ai::agent::{
    AgentConfig as ReActAgentConfig, AgentExecutor as ReActAgentExecutor, SubagentDefLookup,
    SubagentDefSnapshot,
};
use restflow_ai::tools::{Tool, ToolOutput};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};

// Re-export canonical types from restflow-traits (via restflow-ai)
pub use restflow_traits::subagent::{SpawnHandle, SpawnPriority, SpawnRequest, SubagentConfig};

const DEFAULT_SUBAGENT_MAX_ITERATIONS: usize = 20;

/// Spawn a sub-agent with the given request.
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    request: SpawnRequest,
) -> Result<SpawnHandle> {
    let running_count = tracker.running_count();
    if running_count >= config.max_parallel_agents {
        return Err(anyhow!(
            "Max parallel agents ({}) reached",
            config.max_parallel_agents
        ));
    }

    let agent_def = definitions
        .lookup(&request.agent_id)
        .ok_or_else(|| anyhow!("Unknown agent type: {}", request.agent_id))?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = request.timeout_secs.unwrap_or(config.subagent_timeout_secs);

    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();

    let task = request.task.clone();
    let tracker_clone = tracker.clone();
    let task_id_for_spawn = task_id.clone();

    let (completion_tx, completion_rx) = oneshot::channel();
    let (start_tx, start_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let task_id = task_id_for_spawn;
        let _ = start_rx.await;
        let start = std::time::Instant::now();

        let result = timeout(
            Duration::from_secs(timeout_secs),
            execute_subagent(llm_client, tool_registry, agent_def, task.clone()),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let (subagent_result, timed_out) = match result {
            Ok(Ok(output)) => (
                SubagentResult {
                    success: true,
                    output,
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: None,
                },
                false,
            ),
            Ok(Err(e)) => (
                SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some(e.to_string()),
                },
                false,
            ),
            Err(_) => (
                SubagentResult {
                    success: false,
                    output: String::new(),
                    summary: None,
                    duration_ms,
                    tokens_used: None,
                    cost_usd: None,
                    error: Some("Sub-agent timed out".to_string()),
                },
                true,
            ),
        };

        if timed_out {
            tracker_clone.mark_timed_out_with_result(&task_id, subagent_result.clone());
        } else {
            tracker_clone.mark_completed(&task_id, subagent_result.clone());
        }

        let _ = completion_tx.send(subagent_result.clone());
        subagent_result
    });

    tracker.register(
        task_id.clone(),
        agent_name_for_register,
        task_for_register,
        handle,
        completion_rx,
    );

    let _ = start_tx.send(());

    Ok(SpawnHandle {
        id: task_id,
        agent_name: agent_name_for_return,
    })
}

async fn execute_subagent(
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    agent_def: SubagentDefSnapshot,
    task: String,
) -> Result<String> {
    let registry = Arc::new(build_registry_for_agent(
        &tool_registry,
        &agent_def.allowed_tools,
    ));

    let config = ReActAgentConfig::new(task)
        .with_system_prompt(agent_def.system_prompt.clone())
        .with_max_iterations(resolve_max_iterations(&agent_def));
    let engine = ReActAgentExecutor::new(llm_client, registry);
    let result = engine.run(config).await?;
    if result.success {
        Ok(result.answer.unwrap_or_default())
    } else {
        Err(anyhow!(
            "Sub-agent execution failed: {}",
            result.error.unwrap_or_else(|| "unknown error".to_string())
        ))
    }
}

fn resolve_max_iterations(agent_def: &SubagentDefSnapshot) -> usize {
    agent_def
        .max_iterations
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_SUBAGENT_MAX_ITERATIONS)
}

fn build_registry_for_agent(parent: &Arc<ToolRegistry>, allowed_tools: &[String]) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let mut selected = HashSet::new();
    let mut restricted_file_actions = HashSet::new();
    let mut full_file_access = false;

    if allowed_tools.is_empty() {
        for name in parent.list() {
            selected.insert(name.to_string());
        }
    } else {
        for raw in allowed_tools {
            match raw.as_str() {
                "read" => {
                    restricted_file_actions.insert("read".to_string());
                }
                "write" => {
                    restricted_file_actions.insert("write".to_string());
                    restricted_file_actions.insert("list".to_string());
                    restricted_file_actions.insert("search".to_string());
                    restricted_file_actions.insert("exists".to_string());
                }
                "file" => {
                    full_file_access = true;
                    selected.insert("file".to_string());
                }
                other => {
                    selected.insert(normalize_tool_name(other));
                }
            }
        }
    }

    if !full_file_access
        && !restricted_file_actions.is_empty()
        && let Some(file_tool) = parent.get("file")
    {
        registry.register(RestrictedFileTool::new(file_tool, restricted_file_actions));
    }

    for name in selected {
        if let Some(tool) = parent.get(&name) {
            registry.register_arc(tool);
        }
    }

    registry
}

fn normalize_tool_name(name: &str) -> String {
    match name {
        "http_request" => "http".to_string(),
        "send_email" => "email".to_string(),
        "telegram_send" => "telegram".to_string(),
        "grep" => "bash".to_string(),
        other => other.to_string(),
    }
}

#[derive(Clone)]
struct RestrictedFileTool {
    inner: Arc<dyn Tool>,
    allowed_actions: HashSet<String>,
}

impl RestrictedFileTool {
    fn new(inner: Arc<dyn Tool>, allowed_actions: HashSet<String>) -> Self {
        Self {
            inner,
            allowed_actions,
        }
    }

    fn allowed_actions_sorted(&self) -> Vec<String> {
        let mut actions: Vec<String> = self.allowed_actions.iter().cloned().collect();
        actions.sort();
        actions
    }
}

#[async_trait]
impl Tool for RestrictedFileTool {
    fn name(&self) -> &str {
        "file"
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> Value {
        let mut schema = self.inner.parameters_schema();
        if let Some(action_values) = schema
            .pointer_mut("/properties/action/enum")
            .and_then(Value::as_array_mut)
        {
            action_values.retain(|value| {
                value
                    .as_str()
                    .map(|action| self.allowed_actions.contains(action))
                    .unwrap_or(false)
            });
        }
        schema
    }

    async fn execute(&self, input: Value) -> restflow_tools::Result<ToolOutput> {
        let allowed = self.allowed_actions_sorted().join(", ");
        let action = match input.get("action") {
            Some(Value::String(action)) => action,
            Some(_) => {
                return Ok(ToolOutput::error(
                    "Invalid action type. 'action' must be a string.".to_string(),
                ));
            }
            None => {
                return Ok(ToolOutput::error(format!(
                    "Missing required 'action' field. Allowed actions: [{}]",
                    allowed
                )));
            }
        };

        if !self.allowed_actions.contains(action) {
            return Ok(ToolOutput::error(format!(
                "Action '{}' is not allowed for this agent. Allowed actions: [{}]",
                action, allowed
            )));
        }

        self.inner.execute(input).await
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_tools::Result as ToolResult;
    use restflow_ai::tools::ToolOutput;
    use serde_json::{Value, json};

    struct TestTool {
        name: &'static str,
    }

    #[async_trait]
    impl restflow_ai::tools::Tool for TestTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "test tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn execute(&self, _input: Value) -> ToolResult<ToolOutput> {
            Ok(ToolOutput::success(serde_json::json!({"ok": true})))
        }
    }

    #[test]
    fn test_spawn_request_serialization() {
        let request = SpawnRequest {
            agent_id: "researcher".to_string(),
            task: "Research topic X".to_string(),
            timeout_secs: Some(300),
            priority: Some(SpawnPriority::High),
            model: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("researcher"));

        let parsed: SpawnRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "researcher");
    }

    #[test]
    fn test_spawn_handle_serialization() {
        let handle = SpawnHandle {
            id: "task-123".to_string(),
            agent_name: "Researcher".to_string(),
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("task-123"));
    }

    #[test]
    fn test_normalize_tool_name_aliases() {
        assert_eq!(normalize_tool_name("http_request"), "http");
        assert_eq!(normalize_tool_name("send_email"), "email");
        assert_eq!(normalize_tool_name("telegram_send"), "telegram");
        assert_eq!(normalize_tool_name("read"), "read");
        assert_eq!(normalize_tool_name("write"), "write");
        assert_eq!(normalize_tool_name("grep"), "bash");
        assert_eq!(normalize_tool_name("python"), "python");
    }

    #[test]
    fn test_build_registry_for_agent_with_aliases() {
        let mut parent = ToolRegistry::new();
        parent.register(TestTool { name: "file" });
        parent.register(TestTool { name: "bash" });
        parent.register(TestTool { name: "http" });
        let parent = Arc::new(parent);

        let registry = build_registry_for_agent(
            &parent,
            &[
                "read".to_string(),
                "write".to_string(),
                "grep".to_string(),
                "http_request".to_string(),
            ],
        );

        assert!(registry.has("file"));
        assert!(registry.has("bash"));
        assert!(registry.has("http"));
        assert_eq!(registry.list().len(), 3);
    }

    fn test_agent_def_snapshot(max_iterations: Option<u32>) -> SubagentDefSnapshot {
        SubagentDefSnapshot {
            name: "Test".to_string(),
            system_prompt: "You are a test agent".to_string(),
            allowed_tools: vec![],
            max_iterations,
            default_model: None,
        }
    }

    #[test]
    fn test_resolve_max_iterations_uses_agent_override() {
        let definition = test_agent_def_snapshot(Some(42));
        assert_eq!(resolve_max_iterations(&definition), 42);
    }

    #[test]
    fn test_resolve_max_iterations_uses_default_when_missing() {
        let definition = test_agent_def_snapshot(None);
        assert_eq!(
            resolve_max_iterations(&definition),
            DEFAULT_SUBAGENT_MAX_ITERATIONS
        );
    }

    struct MockFileTool;

    #[async_trait]
    impl Tool for MockFileTool {
        fn name(&self) -> &str {
            "file"
        }

        fn description(&self) -> &str {
            "mock file tool"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["read", "write", "list", "search", "exists", "delete"]
                    }
                },
                "required": ["action"]
            })
        }

        async fn execute(&self, input: Value) -> ToolResult<ToolOutput> {
            let action = input
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Ok(ToolOutput::success(json!({ "action": action })))
        }
    }

    #[tokio::test]
    async fn test_restricted_file_tool_blocks_disallowed_actions() {
        let mut allowed_actions = HashSet::new();
        allowed_actions.insert("read".to_string());
        let tool = RestrictedFileTool::new(Arc::new(MockFileTool), allowed_actions);

        let output = tool.execute(json!({ "action": "write" })).await.unwrap();
        assert!(!output.success);
        let error = output.error.unwrap();
        assert!(error.contains("Action 'write' is not allowed"));
    }

    #[tokio::test]
    async fn test_restricted_file_tool_allows_permitted_actions() {
        let mut allowed_actions = HashSet::new();
        allowed_actions.insert("read".to_string());
        let tool = RestrictedFileTool::new(Arc::new(MockFileTool), allowed_actions);

        let output = tool.execute(json!({ "action": "read" })).await.unwrap();
        assert!(output.success);
        assert_eq!(output.result["action"], "read");
    }

    #[tokio::test]
    async fn test_restricted_file_tool_rejects_missing_action() {
        let mut allowed_actions = HashSet::new();
        allowed_actions.insert("read".to_string());
        let tool = RestrictedFileTool::new(Arc::new(MockFileTool), allowed_actions);

        let output = tool.execute(json!({})).await.unwrap();
        assert!(!output.success);
        let error = output.error.unwrap();
        assert!(error.contains("Missing required 'action' field"));
    }

    #[tokio::test]
    async fn test_restricted_file_tool_rejects_non_string_action() {
        let mut allowed_actions = HashSet::new();
        allowed_actions.insert("read".to_string());
        let tool = RestrictedFileTool::new(Arc::new(MockFileTool), allowed_actions);

        let output = tool.execute(json!({ "action": 123 })).await.unwrap();
        assert!(!output.success);
        let error = output.error.unwrap();
        assert!(error.contains("Invalid action type"));
    }

    #[test]
    fn test_restricted_file_tool_schema_is_filtered() {
        let mut allowed_actions = HashSet::new();
        allowed_actions.insert("read".to_string());
        allowed_actions.insert("exists".to_string());
        let tool = RestrictedFileTool::new(Arc::new(MockFileTool), allowed_actions);

        let schema = tool.parameters_schema();
        let action_enum = schema["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(action_enum, vec!["read", "exists"]);
    }

    #[tokio::test]
    async fn test_researcher_cannot_write_files() {
        let mut parent = ToolRegistry::new();
        parent.register(MockFileTool);
        let parent = Arc::new(parent);

        let registry = build_registry_for_agent(&parent, &["read".to_string()]);
        let output = registry
            .execute("file", json!({ "action": "write" }))
            .await
            .unwrap();

        assert!(!output.success);
        let error = output.error.unwrap();
        assert!(error.contains("not allowed"));
    }

    #[tokio::test]
    async fn test_coder_can_read_and_write() {
        let mut parent = ToolRegistry::new();
        parent.register(MockFileTool);
        let parent = Arc::new(parent);

        let registry =
            build_registry_for_agent(&parent, &["read".to_string(), "write".to_string()]);
        let read_output = registry
            .execute("file", json!({ "action": "read" }))
            .await
            .unwrap();
        let write_output = registry
            .execute("file", json!({ "action": "write" }))
            .await
            .unwrap();

        assert!(read_output.success);
        assert!(write_output.success);
    }

    #[tokio::test]
    async fn test_full_file_access_is_unrestricted() {
        let mut parent = ToolRegistry::new();
        parent.register(MockFileTool);
        let parent = Arc::new(parent);

        let registry = build_registry_for_agent(&parent, &["file".to_string()]);
        let output = registry
            .execute("file", json!({ "action": "delete" }))
            .await
            .unwrap();

        assert!(output.success);
        assert_eq!(output.result["action"], "delete");
    }
}
