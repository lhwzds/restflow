//! HTTP request tool.

use crate::runtime::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde_json::{Value, json};

pub struct HttpTool {
    client: reqwest::Client,
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Make HTTP requests (GET, POST, PUT, DELETE)."
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
                    "description": "URL to request"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional headers"
                },
                "body": {
                    "type": "string",
                    "description": "Optional body for POST/PUT"
                }
            },
            "required": ["method", "url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'method' argument".to_string()))?;

        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'url' argument".to_string()))?;

        let headers = args.get("headers").and_then(|v| v.as_object());
        let body = args.get("body").and_then(|v| v.as_str());

        let mut request = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            _ => return Ok(ToolResult::error(format!("Unknown method: {}", method))),
        };

        if let Some(headers) = headers {
            for (key, value) in headers {
                if let Some(value) = value.as_str() {
                    request = request.header(key, value);
                }
            }
        }

        if let Some(body) = body {
            request = request.body(body.to_string());
        }

        let response = request
            .send()
            .await
            .map_err(|e| AiError::Tool(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| AiError::Tool(format!("Failed to read response body: {}", e)))?;

        if status.is_success() {
            Ok(ToolResult::success(json!(text)))
        } else {
            Ok(ToolResult {
                success: false,
                result: json!(text),
                error: Some(format!("HTTP error: {}", status)),
            })
        }
    }
}
