//! HTTP request tool for web interactions.

use super::{Tool, ToolDefinition, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

pub struct HttpTool {
    client: reqwest::Client,
    timeout_secs: u64,
}

impl HttpTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            timeout_secs: 30,
        }
    }
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "http".to_string(),
            description: "Make HTTP requests. Supports GET, POST, PUT, DELETE methods.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "DELETE"],
                        "description": "HTTP method"
                    },
                    "url": {
                        "type": "string",
                        "description": "The URL to request"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional headers as key-value pairs"
                    },
                    "body": {
                        "type": "string",
                        "description": "Optional request body (for POST/PUT)"
                    }
                },
                "required": ["method", "url"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'method' argument"))?;
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' argument"))?;

        let mut request = match method.to_uppercase().as_str() {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            _ => return Ok(ToolResult::error(format!("Unknown method: {}", method))),
        };

        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    request = request.header(key.as_str(), v);
                }
            }
        }

        if let Some(body) = args.get("body").and_then(|v| v.as_str()) {
            request = request.body(body.to_string());
        }

        let response = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.timeout_secs),
            request.send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Request timed out"))?
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            Ok(ToolResult::success(body))
        } else {
            Ok(ToolResult {
                success: false,
                output: body,
                error: Some(format!("HTTP {}", status)),
            })
        }
    }
}
