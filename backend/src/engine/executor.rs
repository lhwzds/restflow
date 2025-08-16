use crate::core::workflow::{Node, NodeType, Workflow};
use crate::engine::context::ExecutionContext;
use crate::engine::graph::WorkflowGraph;
use crate::node::agent::AgentNode;
use serde_json::Value;
use std::collections::HashMap;
use tokio::task::JoinSet;

pub struct WorkflowExecutor {
    workflow: Workflow,
    graph: WorkflowGraph,
    context: ExecutionContext,
}

impl WorkflowExecutor {
    pub fn new(workflow: Workflow) -> Self {
        let workflow_id = workflow.id.clone();
        let graph = WorkflowGraph::from_workflow(&workflow);
        let context = ExecutionContext::new(workflow_id);
        
        Self { 
            workflow,
            graph,
            context,
        }
    }

    pub async fn execute_workflow(&mut self) -> Result<Value, String> {
        let groups = self.graph.get_parallel_groups()?;
        
        println!("Executing workflow in {} stages", groups.len());
        
        for (group_idx, group) in groups.iter().enumerate() {
            println!("Stage {}: executing {:?}", group_idx + 1, group);
            
            let mut tasks = JoinSet::new();
            
            for node_id in group {
                let node = self.graph.get_node(node_id)
                    .ok_or(format!("Node {} not found", node_id))?
                    .clone();
                
                let deps = self.graph.get_dependencies(node_id);
                for dep_id in &deps {
                    if !self.context.node_outputs.contains_key(dep_id) {
                        return Err(format!("Dependency {} not completed for node {}", dep_id, node_id));
                    }
                }
                
                let mut node_context = self.context.clone();
                let node_id_clone = node_id.clone();
                
                tasks.spawn(async move {
                    let result = Self::execute_node(&node, &mut node_context).await;
                    (node_id_clone, result, node_context)
                });
            }
            
            while let Some(result) = tasks.join_next().await {
                match result {
                    Ok((node_id, Ok(output), node_context)) => {
                        println!("Node {} completed", node_id);
                        
                        self.context.set_node_output(node_id.clone(), output.clone());
                        
                        for (key, value) in node_context.variables {
                            self.context.set_variable(key, value);
                        }
                    }
                    Ok((node_id, Err(err), _)) => {
                        return Err(format!("Node {} failed: {}", node_id, err));
                    }
                    Err(e) => {
                        return Err(format!("Task join error: {}", e));
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "execution_id": self.context.execution_id,
            "status": "completed",
            "results": self.context.node_outputs,
            "variables": self.context.variables
        }))
    }

    async fn execute_node(node: &Node, context: &mut ExecutionContext) -> Result<Value, String> {
        println!("Executing node: {} (type: {:?})", node.id, node.node_type);
        
        let config = context.interpolate_value(&node.config);
        match node.node_type {
            NodeType::ManualTrigger => {
                println!("Manual trigger node {} executed", node.id);
                Ok(serde_json::json!({
                    "status": "triggered",
                    "node_id": node.id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }))
            }
            NodeType::HttpRequest => {
                let url = config["url"].as_str().ok_or("No URL")?;

                let method = config["method"]
                    .as_str()
                    .ok_or("Method missing in config")?;
                let client = reqwest::Client::new();
                let response = match method {
                    "GET" => client
                        .get(url)
                        .send()
                        .await
                        .map_err(|e| format!("Get request failed {e}"))?
                        .text()
                        .await
                        .map_err(|e| format!("Get text failed {e}"))?,
                    "POST" => client
                        .post(url)
                        .send()
                        .await
                        .map_err(|e| format!("Post request failed {e}"))?
                        .text()
                        .await
                        .map_err(|e| format!("Get text failed {e}"))?,
                    _ => return Err(format!("Http method {method} not implemented")),
                };

                Ok(serde_json::json!({
                    "status": 200,
                    "body": response
                }))
            }
            NodeType::Agent => {
                let model = config["model"]
                    .as_str()
                    .ok_or("Model missing in config")?
                    .to_string();

                let prompt = config["prompt"]
                    .as_str()
                    .ok_or("Prompt missing in config")?
                    .to_string();

                let temperature = config["temperature"]
                    .as_f64()
                    .ok_or("Temperature missing in config")?;

                let api_key = config["api_key"].as_str().map(|s| s.to_string());

                let input = config["input"].as_str().unwrap_or("Hello");

                let agent = AgentNode::new(model, prompt, temperature, api_key);

                let response = agent
                    .execute(input)
                    .await
                    .map_err(|e| format!("Agent failed {e}"))?;

                Ok(serde_json::json!({
                    "response": response
                }))
            }
            NodeType::Print => {
                let message = config["message"]
                    .as_str()
                    .unwrap_or("No message provided");

                println!("{}", message);

                Ok(serde_json::json!({
                    "printed": message
                }))
            }
            _ => Err(format!(
                "Node type '{:#?}' is not yet implemented",
                node.node_type
            )),
        }
    }
}
