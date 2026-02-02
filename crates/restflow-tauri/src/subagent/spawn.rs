//! Sub-agent spawning support for tool-based execution.

use super::definition::{AgentDefinition, AgentDefinitionRegistry};
use super::tracker::{SubagentResult, SubagentTracker};
use anyhow::{anyhow, Result};
use restflow_ai::llm::CompletionRequest;
use restflow_ai::{LlmClient, Message};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
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
            execute_subagent(llm_client, agent_def, task.clone()),
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
    agent_def: AgentDefinition,
    task: String,
) -> Result<String> {
    let system_prompt = format!(
        "{}\n\n## Your Task\n{}\n\n## Important\
         You are a sub-agent focused on this specific task. \
         Complete it thoroughly and return your results.",
        agent_def.system_prompt, task
    );

    let messages = vec![Message::system(system_prompt), Message::user(task)];

    let request = CompletionRequest::new(messages).with_max_tokens(4096);

    let response = llm_client.complete(request).await?;

    Ok(response.content.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
