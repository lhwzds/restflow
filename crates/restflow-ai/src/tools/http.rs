//! HTTP request tool for making API calls

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::ToolAction;
use crate::error::Result;
use crate::http_client::build_http_client;
use crate::security::SecurityGate;
use crate::tools::traits::check_security;
use crate::tools::traits::{Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct HttpInput {
    method: String,
    url: String,
    headers: Option<Value>,
    body: Option<Value>,
}

/// HTTP request tool for making API calls
pub struct HttpTool {
    client: Client,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTool {
    /// Create a new HTTP tool with default client
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    /// Create with a custom reqwest client
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Send HTTP requests with method, URL, optional headers/body, and return status and response data."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE"],
                    "description": "HTTP method"
                },
                "url": {
                    "type": "string",
                    "description": "Full URL to request"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers"
                },
                "body": {
                    "type": "object",
                    "description": "Optional request body (for POST/PUT)"
                }
            },
            "required": ["method", "url"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: HttpInput = match serde_json::from_value(input) {
            Ok(params) => params,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Invalid input: {}. Required fields: url (string), method (GET|POST|PUT|DELETE|PATCH|HEAD), optional: headers, body, timeout_seconds.",
                    e
                )));
            }
        };

        let action = ToolAction {
            tool_name: "http".to_string(),
            operation: params.method.to_lowercase(),
            target: params.url.clone(),
            summary: format!("HTTP {} {}", params.method.to_uppercase(), params.url),
        };

        if let Some(message) = check_security(
            self.security_gate.as_deref(),
            action,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::error(message));
        }

        let mut request = match params.method.to_uppercase().as_str() {
            "GET" => self.client.get(&params.url),
            "POST" => self.client.post(&params.url),
            "PUT" => self.client.put(&params.url),
            "DELETE" => self.client.delete(&params.url),
            _ => {
                return Ok(ToolOutput::error(format!(
                    "Unknown method: {}",
                    params.method
                )));
            }
        };

        // Add headers (block sensitive headers that could leak credentials)
        const BLOCKED_HEADERS: &[&str] = &[
            "authorization",
            "proxy-authorization",
            "cookie",
            "set-cookie",
        ];
        if let Some(headers) = params.headers
            && let Some(obj) = headers.as_object()
        {
            for (key, value) in obj {
                if BLOCKED_HEADERS.contains(&key.to_ascii_lowercase().as_str()) {
                    tracing::warn!(header = %key, "Blocked sensitive header in HTTP tool");
                    continue;
                }
                if let Some(v) = value.as_str() {
                    request = request.header(key, v);
                }
            }
        }

        // Add body
        if let Some(body) = params.body {
            request = request.json(&body);
        }

        // Execute request
        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();

                // Try to parse as JSON, fallback to string
                let result = serde_json::from_str::<Value>(&body)
                    .unwrap_or_else(|_| json!({ "text": body }));

                Ok(ToolOutput::success(json!({
                    "status": status,
                    "body": result
                })))
            }
            Err(e) => Ok(ToolOutput::error(format!(
                "HTTP request failed: {}. Check that the URL is correct and the server is reachable. For HTTPS issues, verify the certificate is valid.",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_tool_schema() {
        let tool = HttpTool::new();
        assert_eq!(tool.name(), "http_request");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_http_tool_invalid_input_returns_actionable_error() {
        let tool = HttpTool::new();
        let output = tool.execute(json!({"method": "GET"})).await.unwrap();

        assert!(!output.success);
        assert!(output
            .error
            .unwrap_or_default()
            .contains("Required fields: url (string), method (GET|POST|PUT|DELETE|PATCH|HEAD), optional: headers, body, timeout_seconds."));
    }
}
