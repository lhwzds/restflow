//! list_agents tool - List available agent types and running agents.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_traits::SubagentManager;

/// Parameters for list_agents tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsParams {
    /// Include currently running agents in the response.
    #[serde(default = "default_include_running")]
    pub include_running: bool,
}

fn default_include_running() -> bool {
    true
}

/// list_agents tool for the shared agent execution engine.
pub struct ListAgentsTool {
    manager: Arc<dyn SubagentManager>,
}

impl ListAgentsTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for ListAgentsTool {
    fn name(&self) -> &str {
        "list_agents"
    }

    fn description(&self) -> &str {
        "List available agent types and currently running agents."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_running": {
                    "type": "boolean",
                    "default": true,
                    "description": "Include currently running agents"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ListAgentsParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        let available: Vec<Value> = self
            .manager
            .list_callable()
            .iter()
            .map(|def| {
                json!({
                    "id": def.id,
                    "name": def.name,
                    "description": def.description,
                    "tags": def.tags
                })
            })
            .collect();

        let mut response = json!({ "available_agents": available });

        if params.include_running {
            let running: Vec<Value> = self
                .manager
                .list_running()
                .iter()
                .map(|state| {
                    json!({
                        "task_id": state.id,
                        "agent": state.agent_name,
                        "task": state.task,
                        "status": format!("{:?}", state.status),
                        "started_at": state.started_at
                    })
                })
                .collect();

            response["running_agents"] = json!(running);
            response["running_count"] = json!(self.manager.running_count());
        }

        Ok(ToolOutput::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use restflow_ai::agent::{
        SpawnRequest, SubagentConfig, SubagentDefLookup, SubagentDefSnapshot,
        SubagentDefSummary, SubagentDeps, SubagentManagerImpl, SubagentTracker, spawn_subagent,
    };
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use restflow_traits::SubagentManager;
    use std::collections::HashMap;
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

        fn empty() -> Self {
            Self {
                defs: HashMap::new(),
                summaries: Vec::new(),
            }
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

    fn make_deps(
        lookup: MockDefLookup,
        mock_steps: Vec<MockStep>,
    ) -> Arc<SubagentDeps> {
        let (tx, rx) = mpsc::channel(16);
        let tracker = Arc::new(SubagentTracker::new(tx, rx));
        let definitions: Arc<dyn SubagentDefLookup> = Arc::new(lookup);
        let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
        let tool_registry = Arc::new(ToolRegistry::new());
        let config = SubagentConfig {
            max_parallel_agents: 5,
            subagent_timeout_secs: 10,
            max_iterations: 5,
            max_depth: 1,
        };
        Arc::new(SubagentDeps {
            tracker,
            definitions,
            llm_client,
            tool_registry,
            config,
            llm_client_factory: None,
        })
    }

    fn as_manager(deps: &Arc<SubagentDeps>) -> Arc<dyn SubagentManager> {
        Arc::new(SubagentManagerImpl::from_deps(deps))
    }

    #[test]
    fn test_params_default() {
        let params: ListAgentsParams = serde_json::from_str("{}").unwrap();
        assert!(params.include_running);
    }

    #[test]
    fn test_params_no_running() {
        let params: ListAgentsParams =
            serde_json::from_str(r#"{"include_running": false}"#).unwrap();
        assert!(!params.include_running);
    }

    #[tokio::test]
    async fn test_list_with_definitions() {
        let deps = make_deps(
            MockDefLookup::with_agents(vec![
                ("researcher", "Researcher"),
                ("coder", "Coder"),
                ("reviewer", "Reviewer"),
            ]),
            vec![],
        );
        let tool = ListAgentsTool::new(as_manager(&deps));
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        let agents = result.result["available_agents"].as_array().unwrap();
        assert_eq!(agents.len(), 3);
    }

    #[tokio::test]
    async fn test_list_no_running() {
        let deps = make_deps(
            MockDefLookup::with_agents(vec![("coder", "Coder")]),
            vec![],
        );
        let tool = ListAgentsTool::new(as_manager(&deps));
        let result = tool
            .execute(json!({"include_running": false}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.result.get("running_agents").is_none());
    }

    #[tokio::test]
    async fn test_list_with_running_agent() {
        // Use a delayed response so the agent is still running when we list
        let deps = make_deps(
            MockDefLookup::with_agents(vec![("coder", "Coder")]),
            vec![MockStep::text("slow").with_delay(5000)],
        );

        // Spawn an agent that will be slow
        let _handle = spawn_subagent(
            deps.tracker.clone(),
            deps.definitions.clone(),
            deps.llm_client.clone(),
            deps.tool_registry.clone(),
            deps.config.clone(),
            SpawnRequest {
                agent_id: "coder".to_string(),
                task: "write code".to_string(),
                timeout_secs: Some(30),
                priority: None,
                model: None,
            },
            None,
        )
        .unwrap();

        // Small delay to let the agent register
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let tool = ListAgentsTool::new(as_manager(&deps));
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.result["running_count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_list_empty_definitions() {
        let deps = make_deps(MockDefLookup::empty(), vec![]);
        let tool = ListAgentsTool::new(as_manager(&deps));
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        let agents = result.result["available_agents"].as_array().unwrap();
        assert_eq!(agents.len(), 0);
    }
}
