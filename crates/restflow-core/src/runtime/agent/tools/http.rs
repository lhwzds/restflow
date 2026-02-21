//! HTTP request tool with SSRF protection.

use crate::runtime::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::security::resolve_and_validate_url;
use restflow_ai::tools::{Tool, ToolErrorCategory};
use serde_json::{Value, json};

pub struct HttpTool {}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTool {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Make HTTP requests (GET, POST, PUT, DELETE). Validates URLs to prevent SSRF attacks."
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
                    "description": "URL to request (localhost and private IPs are blocked)"
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

        // Resolve DNS and validate all IPs to prevent SSRF
        let (parsed_url, pinned_addr) = match resolve_and_validate_url(url).await {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    result: json!(null),
                    error: Some(format!("URL validation failed: {}", e)),
                    error_category: Some(ToolErrorCategory::Config),
                    retryable: Some(false),
                    retry_after_ms: None,
                });
            }
        };

        let headers = args.get("headers").and_then(|v| v.as_object());
        let body = args.get("body").and_then(|v| v.as_str());

        // Build SSRF-safe client with DNS pinning and no auto-redirects
        let host = parsed_url.host_str().unwrap_or_default();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .resolve(host, pinned_addr)
            .build()
            .map_err(|e| AiError::Tool(format!("Failed to build HTTP client: {}", e)))?;

        let mut request = match method {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
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
            let retryable = status.as_u16() >= 500;
            Ok(ToolResult {
                success: false,
                result: json!(text),
                error: Some(format!("HTTP error: {}", status)),
                error_category: Some(ToolErrorCategory::Network),
                retryable: Some(retryable),
                retry_after_ms: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use restflow_ai::security::validate_url;

    #[test]
    fn test_url_validation_localhost_blocked() {
        assert!(validate_url("http://localhost/admin").is_err());
        assert!(validate_url("http://127.0.0.1/admin").is_err());
        assert!(validate_url("http://0.0.0.0/admin").is_err());
        assert!(validate_url("http://[::1]/admin").is_err());
    }

    #[test]
    fn test_url_validation_private_ip_blocked() {
        // 10.0.0.0/8
        assert!(validate_url("http://10.0.0.1/").is_err());
        assert!(validate_url("http://10.255.255.255/").is_err());
        // 172.16.0.0/12
        assert!(validate_url("http://172.16.0.1/").is_err());
        assert!(validate_url("http://172.31.255.255/").is_err());
        // 192.168.0.0/16
        assert!(validate_url("http://192.168.1.1/").is_err());
        assert!(validate_url("http://192.168.0.1/").is_err());
    }

    #[test]
    fn test_url_validation_link_local_blocked() {
        // 169.254.0.0/16 (includes AWS metadata)
        assert!(validate_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_url("http://169.254.1.1/").is_err());
    }

    #[test]
    fn test_url_validation_invalid_scheme_blocked() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com/file").is_err());
        assert!(validate_url("gopher://example.com/").is_err());
    }

    #[test]
    fn test_url_validation_public_ip_allowed() {
        assert!(validate_url("http://8.8.8.8/").is_ok());
        assert!(validate_url("http://1.1.1.1/").is_ok());
        assert!(validate_url("https://example.com/").is_ok());
        assert!(validate_url("https://api.github.com/").is_ok());
    }

    #[test]
    fn test_url_validation_multicast_blocked() {
        assert!(validate_url("http://224.0.0.1/").is_err());
        assert!(validate_url("http://239.255.255.255/").is_err());
    }
}
