use crate::core::workflow::{Node, NodeType, Workflow};
use crate::node::agent::AgentNode;
use serde_json::Value;
use std::collections::HashMap;

pub struct WorkflowExecutor {
    workflow: Workflow,
}

impl WorkflowExecutor {
    pub fn new(workflow: Workflow) -> Self {
        Self { workflow }
    }

    pub async fn execute_workflow(&self) -> Result<Value, String> {
        let mut results = HashMap::new();

        for node in &self.workflow.nodes {
            println!("Execute node: {}", node.id);
            let result = self.execute_node(node).await?;
            results.insert(node.id.clone(), result);
        }

        Ok(serde_json::json!({
            "status": "completed",
            "results": results
        }))
    }

    pub async fn execute_node(&self, node: &Node) -> Result<Value, String> {
        match node.node_type {
            NodeType::HttpRequest => {
                let url = node.config["url"].as_str().ok_or("No URL")?;

                let method = node.config["method"]
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
                let model = node.config["model"]
                    .as_str()
                    .ok_or("Model missing in config")?
                    .to_string();

                let prompt = node.config["prompt"]
                    .as_str()
                    .ok_or("Prompt missing in config")?
                    .to_string();

                let temperature = node.config["temperature"]
                    .as_f64()
                    .ok_or("Temperature missing in config")?;

                let api_key = node.config["api_key"].as_str().map(|s| s.to_string());

                let input = node.config["input"].as_str().unwrap_or("Hello");

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
                let message = node.config["message"]
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
