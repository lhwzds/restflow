//! Sub-agent spawning with Tokio for parallel execution.
//!
//! This module handles spawning sub-agents that run in parallel,
//! with timeout support and completion notifications.

use super::definition::{AgentDefinition, AgentDefinitionRegistry};
use super::events::{MainAgentEvent, MainAgentEventEmitter, MainAgentEventKind};
use super::tracker::{SubagentResult, SubagentTracker};
use super::MainAgentConfig;
use anyhow::{anyhow, Result};
use restflow_ai::llm::CompletionRequest;
use restflow_ai::{LlmClient, Message};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use ts_rs::TS;

/// Request to spawn a sub-agent
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnRequest {
    /// Agent type ID (e.g., "researcher", "coder")
    pub agent_id: String,

    /// Task description for the agent
    pub task: String,

    /// Optional timeout in seconds
    pub timeout_secs: Option<u64>,

    /// Optional priority level
    pub priority: Option<SpawnPriority>,
}

/// Priority level for sub-agent spawning
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub enum SpawnPriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Handle returned after spawning a sub-agent
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnHandle {
    /// Unique task ID
    pub id: String,

    /// Agent name
    pub agent_name: String,
}

/// Spawn a sub-agent with the given request
pub fn spawn_subagent(
    tracker: Arc<SubagentTracker>,
    definitions: Arc<AgentDefinitionRegistry>,
    llm_client: Arc<dyn LlmClient>,
    event_emitter: Arc<dyn MainAgentEventEmitter>,
    session_id: String,
    config: MainAgentConfig,
    request: SpawnRequest,
) -> Result<SpawnHandle> {
    // Check parallel limit
    let running_count = tracker.running_count();
    if running_count >= config.max_parallel_agents {
        return Err(anyhow!(
            "Max parallel agents ({}) reached",
            config.max_parallel_agents
        ));
    }

    // Get agent definition
    let agent_def = definitions
        .get(&request.agent_id)
        .ok_or_else(|| anyhow!("Unknown agent type: {}", request.agent_id))?
        .clone();

    // Generate unique task ID
    let task_id = uuid::Uuid::new_v4().to_string();
    let timeout_secs = request.timeout_secs.unwrap_or(config.subagent_timeout_secs);

    // Clone values for use after spawn
    let agent_name_for_register = agent_def.name.clone();
    let agent_name_for_return = agent_def.name.clone();
    let task_for_register = request.task.clone();

    // Clone values for the async task
    let task = request.task.clone();
    let agent_name = agent_def.name.clone();
    let _agent_id = request.agent_id.clone();
    let sid = task_id.clone();
    let session_id_for_task = session_id.clone();
    let tracker_clone = tracker.clone();
    let emitter_clone = event_emitter.clone();

    // Register state before spawning to avoid completion races
    tracker.register_state(
        task_id.clone(),
        agent_name_for_register,
        task_for_register,
    );

    // Spawn the Tokio task
    let handle = tokio::spawn(async move {
        let start = std::time::Instant::now();

        // Emit started event
        emitter_clone.emit(MainAgentEvent {
            session_id: session_id_for_task.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::SubagentSpawned {
                task_id: sid.clone(),
                agent_name: agent_name.clone(),
                task: task.clone(),
            },
        });

        // Execute with timeout
        let result = timeout(
            Duration::from_secs(timeout_secs),
            execute_subagent(
                llm_client,
                agent_def,
                task.clone(),
                emitter_clone.clone(),
                sid.clone(),
                session_id_for_task.clone(),
            ),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let (subagent_result, timed_out) = match result {
            Ok(Ok(output)) => (
                SubagentResult {
                    success: true,
                    output,
                    summary: None, // TODO: Generate summary
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
            tracker_clone.mark_timed_out_with_result(&sid, subagent_result.clone());
        } else {
            tracker_clone.mark_completed(&sid, subagent_result.clone());
        }

        // Emit completed event
        emitter_clone.emit(MainAgentEvent {
            session_id: session_id_for_task.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: MainAgentEventKind::SubagentCompleted {
                task_id: sid.clone(),
                agent_name: agent_name.clone(),
                success: subagent_result.success,
                summary: subagent_result.summary.clone(),
                duration_ms: subagent_result.duration_ms,
            },
        });

        subagent_result
    });

    // Attach handle after spawn
    tracker.attach_handle(task_id.clone(), handle);

    Ok(SpawnHandle {
        id: task_id,
        agent_name: agent_name_for_return,
    })
}

/// Execute a sub-agent's task
async fn execute_subagent(
    llm_client: Arc<dyn LlmClient>,
    agent_def: AgentDefinition,
    task: String,
    event_emitter: Arc<dyn MainAgentEventEmitter>,
    task_id: String,
    session_id: String,
) -> Result<String> {
    // Build system prompt for sub-agent
    let system_prompt = format!(
        "{}\n\n## Your Task\n{}\n\n## Important\n\
         You are a sub-agent focused on this specific task. \
         Complete it thoroughly and return your results.",
        agent_def.system_prompt, task
    );

    // Emit progress event
    event_emitter.emit(MainAgentEvent {
        session_id: session_id.clone(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        kind: MainAgentEventKind::SubagentProgress {
            task_id: task_id.clone(),
            agent_name: agent_def.name.clone(),
            step: "Starting execution".to_string(),
        },
    });

    // TODO: Implement full ReAct loop with the sub-agent
    // For now, we'll make a simple LLM call
    let messages = vec![
        Message::system(system_prompt),
        Message::user(task),
    ];

    let request = CompletionRequest::new(messages).with_max_tokens(4096);

    let response = llm_client.complete(request).await?;

    // Emit completion progress
    event_emitter.emit(MainAgentEvent {
        session_id,
        timestamp: chrono::Utc::now().timestamp_millis(),
        kind: MainAgentEventKind::SubagentProgress {
            task_id,
            agent_name: agent_def.name,
            step: "Completed".to_string(),
        },
    });

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
