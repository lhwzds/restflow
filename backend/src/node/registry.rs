use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use anyhow::Result;
use serde_json::Value;
use crate::models::NodeType;
use crate::engine::context::ExecutionContext;

#[async_trait]
pub trait NodeExecutor: Send + Sync {
    async fn execute(&self, config: &Value, context: &mut ExecutionContext) -> Result<Value>;
}

pub struct NodeRegistry {
    executors: HashMap<NodeType, Arc<dyn NodeExecutor>>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            executors: HashMap::new(),
        };
        
        registry.register(NodeType::ManualTrigger, Arc::new(ManualTriggerExecutor));
        registry.register(NodeType::WebhookTrigger, Arc::new(WebhookTriggerExecutor));
        registry.register(NodeType::ScheduleTrigger, Arc::new(ScheduleTriggerExecutor));
        registry.register(NodeType::HttpRequest, Arc::new(HttpRequestExecutor));
        registry.register(NodeType::Print, Arc::new(PrintExecutor));
        registry.register(NodeType::Agent, Arc::new(AgentExecutor));
        
        registry
    }
    
    pub fn register(&mut self, node_type: NodeType, executor: Arc<dyn NodeExecutor>) {
        self.executors.insert(node_type, executor);
    }
    
    pub fn get(&self, node_type: &NodeType) -> Option<Arc<dyn NodeExecutor>> {
        self.executors.get(node_type).cloned()
    }
}

struct ManualTriggerExecutor;

#[async_trait]
impl NodeExecutor for ManualTriggerExecutor {
    async fn execute(&self, _config: &Value, _context: &mut ExecutionContext) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "manual",
            "triggered_at": chrono::Utc::now().to_rfc3339()
        }))
    }
}

struct WebhookTriggerExecutor;

#[async_trait]
impl NodeExecutor for WebhookTriggerExecutor {
    async fn execute(&self, _config: &Value, _context: &mut ExecutionContext) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "webhook",
            "triggered_at": chrono::Utc::now().to_rfc3339()
        }))
    }
}

struct ScheduleTriggerExecutor;

#[async_trait]
impl NodeExecutor for ScheduleTriggerExecutor {
    async fn execute(&self, _config: &Value, _context: &mut ExecutionContext) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "schedule",
            "triggered_at": chrono::Utc::now().to_rfc3339()
        }))
    }
}

struct HttpRequestExecutor;

#[async_trait]
impl NodeExecutor for HttpRequestExecutor {
    async fn execute(&self, config: &Value, _context: &mut ExecutionContext) -> Result<Value> {
        let url = config["url"].as_str().ok_or_else(|| anyhow::anyhow!("URL not found in config"))?;
        let method = config["method"].as_str().unwrap_or("GET");
        
        let client = reqwest::Client::new();
        let response = match method {
            "GET" => self.send_get(client, url).await?,
            "POST" => self.send_post(client, url).await?,
            _ => return Err(anyhow::anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        Ok(serde_json::json!({
            "status": 200,
            "body": response
        }))
    }
}

impl HttpRequestExecutor {
    async fn send_get(&self, client: reqwest::Client, url: &str) -> Result<String> {
        client.get(url)
            .send().await
            .and_then(|r| r.error_for_status())
            .map_err(|e| anyhow::anyhow!("GET request failed: {}", e))?
            .text().await
            .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))
    }
    
    async fn send_post(&self, client: reqwest::Client, url: &str) -> Result<String> {
        client.post(url)
            .send().await
            .and_then(|r| r.error_for_status())
            .map_err(|e| anyhow::anyhow!("POST request failed: {}", e))?
            .text().await
            .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))
    }
}

struct PrintExecutor;

#[async_trait]
impl NodeExecutor for PrintExecutor {
    async fn execute(&self, config: &Value, _context: &mut ExecutionContext) -> Result<Value> {
        let message = config["message"].as_str().unwrap_or("No message provided");
        println!("{}", message);
        
        Ok(serde_json::json!({
            "printed": message
        }))
    }
}

struct AgentExecutor;

#[async_trait]
impl NodeExecutor for AgentExecutor {
    async fn execute(&self, config: &Value, context: &mut ExecutionContext) -> Result<Value> {
        use crate::node::agent::AgentNode;

        let agent = AgentNode::from_config(config)?;
        let input = config["input"].as_str().unwrap_or("Hello");

        let secret_storage = context.secret_storage.as_ref().map(|s| s.as_ref());
        let response = agent.execute(input, secret_storage).await
            .map_err(|e| anyhow::anyhow!("Agent execution failed: {}", e))?;
        
        Ok(serde_json::json!({
            "response": response
        }))
    }
}