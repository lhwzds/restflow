//! HTTP request tool for making API calls.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::http_client::build_ssrf_safe_client;
use crate::security::{NetworkAllowlist, resolve_and_validate_url};
use crate::security::{SecurityGate, ToolAction};
use crate::{Tool, ToolErrorCategory, ToolOutput, check_security};

#[derive(Debug, Deserialize)]
struct HttpInput {
    method: String,
    url: String,
    headers: Option<Value>,
    body: Option<Value>,
}

/// HTTP request tool for making API calls.
pub struct HttpTool {
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
    network_allowlist: Option<NetworkAllowlist>,
}

impl HttpTool {
    pub fn new() -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            security_gate: None,
            agent_id: None,
            task_id: None,
            network_allowlist: None,
        })
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

    pub fn with_network_allowlist(mut self, allowlist: NetworkAllowlist) -> Self {
        self.network_allowlist = Some(allowlist);
        self
    }

    fn check_network_allowlist(&self, url: &str) -> std::result::Result<(), String> {
        if let Some(ref allowlist) = self.network_allowlist {
            let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
            let host = parsed
                .host_str()
                .ok_or_else(|| "URL has no host".to_string())?;

            if !allowlist.is_host_allowed(host) {
                return Err(format!(
                    "URL host '{}' is not in the allowed network list. Allowed domains: {:?}",
                    host,
                    allowlist.allowed_domains()
                ));
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

        let (parsed_url, pinned_addr) = match resolve_and_validate_url(&params.url).await {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolOutput::non_retryable_error(
                    format!("URL validation failed: {}", e),
                    ToolErrorCategory::Config,
                ));
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
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }

        if let Err(e) = self.check_network_allowlist(&params.url) {
            return Ok(ToolOutput::non_retryable_error(e, ToolErrorCategory::Auth));
        }

        let host = parsed_url.host_str().unwrap_or_default();
        let client = build_ssrf_safe_client(host, pinned_addr).map_err(anyhow::Error::from)?;

        let max_redirects = 5;
        let mut current_url = params.url.clone();
        let mut current_client = client;

        for hop in 0..=max_redirects {
            let mut request = match params.method.to_uppercase().as_str() {
                "GET" => current_client.get(&current_url),
                "POST" => current_client.post(&current_url),
                "PUT" => current_client.put(&current_url),
                "DELETE" => current_client.delete(&current_url),
                _ => {
                    return Ok(ToolOutput::non_retryable_error(
                        format!("Unknown method: {}", params.method),
                        ToolErrorCategory::Config,
                    ));
                }
            };

            const BLOCKED_HEADERS: &[&str] = &[
                "authorization",
                "proxy-authorization",
                "cookie",
                "set-cookie",
            ];
            if let Some(ref headers) = params.headers
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

            if hop == 0
                && let Some(ref body) = params.body
            {
                request = request.json(body);
            }

            match request.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if (301..=308).contains(&status) {
                        if hop >= max_redirects {
                            return Ok(ToolOutput::non_retryable_error(
                                format!("Too many redirects (max {})", max_redirects),
                                ToolErrorCategory::Network,
                            ));
                        }

                        let location = match response.headers().get("location") {
                            Some(loc) => loc.to_str().unwrap_or_default().to_string(),
                            None => {
                                return Ok(ToolOutput::non_retryable_error(
                                    "Redirect without Location header".to_string(),
                                    ToolErrorCategory::Network,
                                ));
                            }
                        };

                        let redirect_url = match url::Url::parse(&location) {
                            Ok(u) => u.to_string(),
                            Err(_) => {
                                let base = url::Url::parse(&current_url).unwrap();
                                match base.join(&location) {
                                    Ok(u) => u.to_string(),
                                    Err(_) => location.clone(),
                                }
                            }
                        };

                        let (new_parsed, new_addr) =
                            match resolve_and_validate_url(&redirect_url).await {
                                Ok(v) => v,
                                Err(e) => {
                                    return Ok(ToolOutput::non_retryable_error(
                                        format!("Redirect target validation failed: {}", e),
                                        ToolErrorCategory::Config,
                                    ));
                                }
                            };

                        let new_host = new_parsed.host_str().unwrap_or_default();
                        current_client = build_ssrf_safe_client(new_host, new_addr)
                            .map_err(anyhow::Error::from)?;
                        current_url = redirect_url;
                        continue;
                    }

                    let headers = response.headers().clone();
                    let body = response.text().await.unwrap_or_default();

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

                    return Ok(ToolOutput::success(json!({
                        "status": status,
                        "body": result
                    })));
                }
                Err(e) => {
                    return Ok(ToolOutput::retryable_error(
                        format!(
                            "HTTP request failed: {}. Check that the URL is correct and the server is reachable.",
                            e
                        ),
                        ToolErrorCategory::Network,
                    ));
                }
            }
        }

        Ok(ToolOutput::non_retryable_error(
            format!("Too many redirects (max {})", max_redirects),
            ToolErrorCategory::Network,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::validate_url;

    #[test]
    fn test_http_tool_schema() {
        let tool = HttpTool::new().unwrap();
        assert_eq!(tool.name(), "http_request");
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
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
    }

    #[test]
    fn test_url_validation_localhost_blocked() {
        assert!(validate_url("http://localhost/admin").is_err());
        assert!(validate_url("http://127.0.0.1/admin").is_err());
    }

    #[test]
    fn test_url_validation_public_allowed() {
        assert!(validate_url("https://example.com/").is_ok());
        assert!(validate_url("http://8.8.8.8/").is_ok());
    }
}
