use crate::core::workflow::{Node, NodeType, Workflow};
use serde_json::Value;
use std::collections::HashMap;

pub struct WorkflowExecutor {
    workflow: Workflow,
}

impl WorkflowExecutor {
    pub fn new(workflow: Workflow) -> Self {
        Self { workflow }
    }

    pub async fn execute(&self) -> Result<Value, String> {
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
            _ => Err(format!(
                "Node type '{:#?}' is not yet implemented",
                node.node_type
            )),
        }
    }
}
