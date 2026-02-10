//! Sub-agent spawning support for tool-based execution.

use super::definition::{AgentDefinition, AgentDefinitionRegistry};
use super::tracker::{SubagentResult, SubagentTracker};
use crate::runtime::agent::{AgentExecutionEngine, AgentExecutionEngineConfig, ToolRegistry};
use anyhow::{Result, anyhow};
use restflow_ai::LlmClient;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};
use ts_rs::TS;

/// Configuration for sub-agent execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentConfig {
    /// Maximum number of parallel sub-agents.
    pub max_parallel_agents: usize,
    /// Default timeout for sub-agents in seconds.
    pub subagent_timeout_secs: u64,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: 5,
            subagent_timeout_secs: 300,
        }
    }
}

/// Request to spawn a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder").
    pub agent_id: String,

    /// Task description for the agent.
    pub task: String,

    /// Optional timeout in seconds.
    pub timeout_secs: Option<u64>,

    /// Optional priority level.
    pub priority: Option<SpawnPriority>,
}

/// Priority level for sub-agent spawning.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Handle returned after spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnHandle {
    /// Unique task ID.
    pub id: String,

    /// Agent name.
    pub agent_name: String,
}

/// Spawn a sub-agent with the given request.
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<AgentDefinitionRegistry>,
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
        .get(&request.agent_id)
        .ok_or_else(|| anyhow!("Unknown agent type: {}", request.agent_id))?
        .clone();

    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = request.timeout_secs.unwrap_or(config.subagent_timeout_secs);

    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();

    let task = request.task.clone();
    let _agent_name = agent_def.name.clone();
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
            execute_subagent(
                llm_client,
                tool_registry,
                agent_def,
                task.clone(),
                config.clone(),
            ),
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
    agent_def: AgentDefinition,
    task: String,
    config: SubagentConfig,
) -> Result<String> {
    let registry = Arc::new(build_registry_for_agent(
        &tool_registry,
        &agent_def.allowed_tools,
    ));

    let mut engine_config = AgentExecutionEngineConfig::default();
    engine_config.react.max_iterations = agent_def
        .max_iterations
        .map(|value| value as usize)
        .unwrap_or(config.max_parallel_agents.max(10));
    let mut engine = AgentExecutionEngine::new(
        llm_client,
        registry,
        agent_def.system_prompt.clone(),
        engine_config,
    );

    let result = engine.execute(&task).await?;
    if result.success {
        Ok(result.output)
    } else {
        Err(anyhow!("Sub-agent execution failed: {}", result.output))
    }
}

fn build_registry_for_agent(parent: &Arc<ToolRegistry>, allowed_tools: &[String]) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let mut selected = HashSet::new();

    if allowed_tools.is_empty() {
        for name in parent.list() {
            selected.insert(name.to_string());
        }
    } else {
        for raw in allowed_tools {
            selected.insert(normalize_tool_name(raw));
        }
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
        "read" | "write" => "file".to_string(),
        "grep" => "bash".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use restflow_ai::error::Result as AiResult;
    use restflow_ai::tools::ToolOutput;
    use serde_json::Value;

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

        async fn execute(&self, _input: Value) -> AiResult<ToolOutput> {
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
        assert_eq!(normalize_tool_name("read"), "file");
        assert_eq!(normalize_tool_name("write"), "file");
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
}
