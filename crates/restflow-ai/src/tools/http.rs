//! HTTP request tool for making API calls

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::SecurityGate;
use crate::ToolAction;
use crate::error::AiError;
use crate::error::Result;
use crate::http_client::build_http_client;
use crate::security::NetworkAllowlist;
use crate::security::validate_url;
use crate::tools::traits::check_security;
use crate::tools::traits::{Tool, ToolErrorCategory, ToolOutput};

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
    network_allowlist: Option<NetworkAllowlist>,
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
            network_allowlist: None,
        }
    }

    /// Create with a custom reqwest client
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            security_gate: None,
            agent_id: None,
            task_id: None,
            network_allowlist: None,
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

    /// Set network allowlist for domain validation
    pub fn with_network_allowlist(mut self, allowlist: NetworkAllowlist) -> Self {
        self.network_allowlist = Some(allowlist);
        self
    }

    /// Check if URL host is allowed by network allowlist
    fn check_network_allowlist(&self, url: &str) -> Result<()> {
        if let Some(ref allowlist) = self.network_allowlist {
            // Parse URL to extract host
            let parsed =
                url::Url::parse(url).map_err(|e| AiError::Tool(format!("Invalid URL: {}", e)))?;

            let host = parsed
                .host_str()
                .ok_or_else(|| AiError::Tool("URL has no host".to_string()))?;

            if !allowlist.is_host_allowed(host) {
                return Err(AiError::Tool(format!(
                    "URL host '{}' is not in the allowed network list. Allowed domains: {:?}",
                    host,
                    allowlist.allowed_domains()
                )));
            }
        }
        Ok(())
    }

    fn classify_status(status: u16) -> (ToolErrorCategory, bool) {
        match status {
            401 | 403 => (ToolErrorCategory::Auth, false),
            404 => (ToolErrorCategory::NotFound, false),
            429 => (ToolErrorCategory::RateLimit, true),
            500..=599 => (ToolErrorCategory::Network, true),
            _ => (ToolErrorCategory::Execution, false),
        }
    }

    fn parse_retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
        headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1000))
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Send HTTP requests with method, URL, optional headers/body, and return status and response data. This is the PRIMARY HTTP tool."
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
                return Ok(ToolOutput::non_retryable_error(
                    format!(
                        "Invalid input: {}. Required fields: url (string), method (GET|POST|PUT|DELETE|PATCH|HEAD), optional: headers, body, timeout_seconds.",
                        e
                    ),
                    ToolErrorCategory::Config,
                ));
            }
        };

        // Validate URL to prevent SSRF attacks
        if let Err(e) = validate_url(&params.url) {
            return Ok(ToolOutput::non_retryable_error(
                format!("URL validation failed: {}", e),
                ToolErrorCategory::Config,
            ));
        }

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
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }

        // Check network allowlist
        if let Err(e) = self.check_network_allowlist(&params.url) {
            return Ok(ToolOutput::non_retryable_error(
                e.to_string(),
                ToolErrorCategory::Auth,
            ));
        }

        let mut request = match params.method.to_uppercase().as_str() {
            "GET" => self.client.get(&params.url),
            "POST" => self.client.post(&params.url),
            "PUT" => self.client.put(&params.url),
            "DELETE" => self.client.delete(&params.url),
            _ => {
                return Ok(ToolOutput::non_retryable_error(
                    format!("Unknown method: {}", params.method),
                    ToolErrorCategory::Config,
                ));
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
                let headers = response.headers().clone();
                let body = response.text().await.unwrap_or_default();

                // Try to parse as JSON, fallback to string
                let result = serde_json::from_str::<Value>(&body)
                    .unwrap_or_else(|_| json!({ "text": body }));

                if status >= 400 {
                    let (category, retryable) = Self::classify_status(status);
                    let retry_after_ms = if status == 429 {
                        Self::parse_retry_after_ms(&headers)
                    } else {
                        None
                    };
                    return Ok(ToolOutput {
                        success: false,
                        result: json!({
                            "status": status,
                            "body": result
                        }),
                        error: Some(format!("HTTP request failed with status {}", status)),
                        error_category: Some(category),
                        retryable: Some(retryable),
                        retry_after_ms,
                    });
                }

                Ok(ToolOutput::success(json!({
                    "status": status,
                    "body": result
                })))
            }
            Err(e) => Ok(ToolOutput::retryable_error(
                format!(
                    "HTTP request failed: {}. Check that the URL is correct and the server is reachable. For HTTPS issues, verify the certificate is valid.",
                    e
                ),
                ToolErrorCategory::Network,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::validate_url;
    use reqwest::header::{HeaderMap, HeaderValue};

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
        assert_eq!(output.error_category, Some(ToolErrorCategory::Config));
        assert_eq!(output.retryable, Some(false));
        assert!(output
            .error
            .unwrap_or_default()
            .contains("Required fields: url (string), method (GET|POST|PUT|DELETE|PATCH|HEAD), optional: headers, body, timeout_seconds."));
    }

    #[test]
    fn test_http_status_classification() {
        assert_eq!(
            HttpTool::classify_status(401),
            (ToolErrorCategory::Auth, false)
        );
        assert_eq!(
            HttpTool::classify_status(404),
            (ToolErrorCategory::NotFound, false)
        );
        assert_eq!(
            HttpTool::classify_status(429),
            (ToolErrorCategory::RateLimit, true)
        );
        assert_eq!(
            HttpTool::classify_status(503),
            (ToolErrorCategory::Network, true)
        );
    }

    #[test]
    fn test_http_retry_after_parse() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("12"));
        assert_eq!(HttpTool::parse_retry_after_ms(&headers), Some(12_000));
    }

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
        // Public IPs should be allowed
        assert!(validate_url("http://8.8.8.8/").is_ok()); // Google DNS
        assert!(validate_url("http://1.1.1.1/").is_ok()); // Cloudflare DNS
        assert!(validate_url("https://example.com/").is_ok());
        assert!(validate_url("https://api.github.com/").is_ok());
    }

    #[test]
    fn test_url_validation_multicast_blocked() {
        assert!(validate_url("http://224.0.0.1/").is_err());
        assert!(validate_url("http://239.255.255.255/").is_err());
    }
}
