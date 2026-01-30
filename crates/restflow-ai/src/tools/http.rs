//! HTTP request tool for making API calls

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Result;
use crate::http_client::build_http_client;
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
        }
    }

    /// Create with a custom reqwest client
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make HTTP requests to external APIs. Supports GET, POST, PUT, DELETE methods."
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
        let params: HttpInput = serde_json::from_value(input)?;

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

        // Add headers
        if let Some(headers) = params.headers
            && let Some(obj) = headers.as_object()
        {
            for (key, value) in obj {
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
            Err(e) => Ok(ToolOutput::error(e.to_string())),
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
}
