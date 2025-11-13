use crate::engine::context::ExecutionContext;
use crate::models::{AgentOutput, HttpOutput, NodeOutput, NodeType, PrintOutput, PythonOutput};
use crate::node::trigger::TriggerExecutor;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait NodeExecutor: Send + Sync {
    async fn execute(
        &self,
        node_type: &NodeType,
        config: &Value,
        context: &mut ExecutionContext,
    ) -> Result<NodeOutput>;
}

pub struct NodeRegistry {
    executors: HashMap<NodeType, Arc<dyn NodeExecutor>>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            executors: HashMap::new(),
        };

        // Register trigger executor (using unified TriggerExecutor)
        let trigger_executor = Arc::new(TriggerExecutor);
        registry.register(NodeType::ManualTrigger, trigger_executor.clone());
        registry.register(NodeType::WebhookTrigger, trigger_executor.clone());
        registry.register(NodeType::ScheduleTrigger, trigger_executor);

        // Register other node executors
        registry.register(NodeType::HttpRequest, Arc::new(HttpRequestExecutor));
        registry.register(NodeType::Print, Arc::new(PrintExecutor));
        registry.register(NodeType::Agent, Arc::new(AgentExecutor));
        registry.register(NodeType::Python, Arc::new(PythonExecutor));

        registry
    }

    pub fn register(&mut self, node_type: NodeType, executor: Arc<dyn NodeExecutor>) {
        self.executors.insert(node_type, executor);
    }

    pub fn get(&self, node_type: &NodeType) -> Option<Arc<dyn NodeExecutor>> {
        self.executors.get(node_type).cloned()
    }
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

struct HttpRequestExecutor;

#[async_trait]
impl NodeExecutor for HttpRequestExecutor {
    async fn execute(
        &self,
        _node_type: &NodeType,
        config: &Value,
        _context: &mut ExecutionContext,
    ) -> Result<NodeOutput> {
        let url = config["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("URL not found in config"))?;
        let method = config["method"].as_str().unwrap_or("GET");

        // Parse timeout (default: 30 seconds)
        let timeout_ms = config["timeout_ms"].as_u64().unwrap_or(30000);
        let timeout = std::time::Duration::from_millis(timeout_ms);

        // Build client with timeout
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))?;

        // Build request
        let mut request_builder = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => return Err(anyhow::anyhow!("Unsupported HTTP method: {}", method)),
        };

        // Add headers if present
        if let Some(headers) = config.get("headers").and_then(|h| h.as_object()) {
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    request_builder = request_builder.header(key, value_str);
                }
            }
        }

        // Add body if present (for POST, PUT, PATCH)
        if matches!(method.to_uppercase().as_str(), "POST" | "PUT" | "PATCH")
            && let Some(body) = config.get("body")
        {
            if body.is_string() {
                // String body
                request_builder = request_builder.body(body.as_str().unwrap().to_string());
            } else {
                // JSON body
                request_builder = request_builder.json(body);
            }
        }

        // Send request
        let response = request_builder
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

        // Extract status and headers
        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Read body
        let body_text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))?;

        // Try to parse as JSON, fallback to string
        let body = serde_json::from_str::<Value>(&body_text).unwrap_or(Value::String(body_text));

        Ok(NodeOutput::Http(HttpOutput {
            status,
            headers,
            body,
        }))
    }
}

struct PrintExecutor;

#[async_trait]
impl NodeExecutor for PrintExecutor {
    async fn execute(
        &self,
        _node_type: &NodeType,
        config: &Value,
        _context: &mut ExecutionContext,
    ) -> Result<NodeOutput> {
        let message = config["message"].as_str().unwrap_or("No message provided");
        println!("{}", message);

        Ok(NodeOutput::Print(PrintOutput {
            printed: message.to_string(),
        }))
    }
}

struct AgentExecutor;

#[async_trait]
impl NodeExecutor for AgentExecutor {
    async fn execute(
        &self,
        _node_type: &NodeType,
        config: &Value,
        context: &mut ExecutionContext,
    ) -> Result<NodeOutput> {
        use crate::node::agent::AgentNode;

        let agent = AgentNode::from_config(config)?;
        let input = config["input"].as_str().unwrap_or("Hello");

        let secret_storage = context.secret_storage.as_ref().map(|s| s.as_ref());
        let response = agent
            .execute(input, secret_storage)
            .await
            .map_err(|e| anyhow::anyhow!("Agent execution failed: {}", e))?;

        Ok(NodeOutput::Agent(AgentOutput { response }))
    }
}

struct PythonExecutor;

#[async_trait]
impl NodeExecutor for PythonExecutor {
    async fn execute(
        &self,
        _node_type: &NodeType,
        config: &Value,
        context: &mut ExecutionContext,
    ) -> Result<NodeOutput> {
        use crate::node::python::PythonNode;

        let python = PythonNode::from_config(config)?;
        let script = python.build_script();

        // Get input from config or use empty object
        let input = config
            .get("input")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        // Get PythonManager from context
        let manager = context
            .python_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Python manager not available"))?;

        // Read common AI API keys from Secret Manager
        let mut env_vars = std::collections::HashMap::new();
        if let Some(secret_storage) = &context.secret_storage {
            // Try to load OPENAI_API_KEY
            if let Ok(Some(key)) = secret_storage.get_secret("OPENAI_API_KEY") {
                env_vars.insert("OPENAI_API_KEY".to_string(), key);
            }
            // Add other AI providers as needed
            if let Ok(Some(key)) = secret_storage.get_secret("ANTHROPIC_API_KEY") {
                env_vars.insert("ANTHROPIC_API_KEY".to_string(), key);
            }
        }

        let result = manager
            .execute_inline_code(&script, input, env_vars)
            .await?;

        Ok(NodeOutput::Python(PythonOutput { result }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_registry_creation() {
        let registry = NodeRegistry::new();

        // Verify all node types are registered
        assert!(registry.get(&NodeType::HttpRequest).is_some());
        assert!(registry.get(&NodeType::Print).is_some());
        assert!(registry.get(&NodeType::Agent).is_some());
        assert!(registry.get(&NodeType::Python).is_some());
        assert!(registry.get(&NodeType::ManualTrigger).is_some());
        assert!(registry.get(&NodeType::WebhookTrigger).is_some());
        assert!(registry.get(&NodeType::ScheduleTrigger).is_some());
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = NodeRegistry::new();

        // All known types should be registered, so we can't easily test nonexistent
        // This test just verifies the registry works
        assert!(registry.get(&NodeType::Print).is_some());
    }

    #[tokio::test]
    async fn test_print_executor_basic() {
        let executor = PrintExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({
            "message": "Hello World"
        });

        let result = executor
            .execute(&NodeType::Print, &config, &mut context)
            .await;

        assert!(result.is_ok());
        if let NodeOutput::Print(output) = result.unwrap() {
            assert_eq!(output.printed, "Hello World");
        } else {
            panic!("Expected Print output");
        }
    }

    #[tokio::test]
    async fn test_print_executor_empty_message() {
        let executor = PrintExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({});

        let result = executor
            .execute(&NodeType::Print, &config, &mut context)
            .await;

        assert!(result.is_ok());
        if let NodeOutput::Print(output) = result.unwrap() {
            assert_eq!(output.printed, "No message provided");
        } else {
            panic!("Expected Print output");
        }
    }

    #[tokio::test]
    async fn test_http_executor_missing_url() {
        let executor = HttpRequestExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({
            "method": "GET"
        });

        let result = executor
            .execute(&NodeType::HttpRequest, &config, &mut context)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("URL not found"));
    }

    #[tokio::test]
    async fn test_http_executor_unsupported_method() {
        let executor = HttpRequestExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({
            "url": "http://example.com",
            "method": "INVALID"
        });

        let result = executor
            .execute(&NodeType::HttpRequest, &config, &mut context)
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported HTTP method")
        );
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn test_http_executor_get_request() {
        let executor = HttpRequestExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({
            "url": "https://httpbin.org/get",
            "method": "GET"
        });

        let result = executor
            .execute(&NodeType::HttpRequest, &config, &mut context)
            .await;

        assert!(result.is_ok());
        if let NodeOutput::Http(output) = result.unwrap() {
            assert_eq!(output.status, 200);
            assert!(output.body.is_object());
        } else {
            panic!("Expected Http output");
        }
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn test_http_executor_post_with_json() {
        let executor = HttpRequestExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({
            "url": "https://httpbin.org/post",
            "method": "POST",
            "body": {"test": "data"},
            "headers": {"Content-Type": "application/json"}
        });

        let result = executor
            .execute(&NodeType::HttpRequest, &config, &mut context)
            .await;

        assert!(result.is_ok());
        if let NodeOutput::Http(output) = result.unwrap() {
            assert_eq!(output.status, 200);
        } else {
            panic!("Expected Http output");
        }
    }

    #[tokio::test]
    async fn test_python_executor_without_manager() {
        let executor = PythonExecutor;
        let mut context = ExecutionContext::new("test".to_string());

        let config = json!({
            "code": "print('test')"
        });

        let result = executor
            .execute(&NodeType::Python, &config, &mut context)
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Python manager not available")
        );
    }
}
