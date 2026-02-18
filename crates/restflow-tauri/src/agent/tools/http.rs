//! HTTP request tool with SSRF protection.

use crate::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::{Tool, ToolErrorCategory};
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv6Addr};

/// Validate URL to prevent SSRF attacks.
/// Blocks access to internal/private network resources.
fn validate_url(url: &str) -> std::result::Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Only allow HTTP and HTTPS schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Scheme '{}' is not allowed. Only HTTP and HTTPS are permitted.",
                scheme
            ))
        }
    }

    // Check host
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return Err("URL must have a host".to_string()),
    };

    // Block localhost variations
    if host.eq_ignore_ascii_case("localhost")
        || host == "0.0.0.0"
        || host == "::1"
        || host == "[::1]"
    {
        return Err("Access to localhost is not allowed".to_string());
    }

    // Try to parse as IP address
    if let Ok(ip) = host.parse::<IpAddr>() && is_restricted_ip(&ip) {
        return Err(format!(
            "Access to restricted IP address {} is not allowed (private/internal/metadata)",
            ip
        ));
    }

    // Handle bracketed IPv6 addresses
    if host.starts_with('[') && host.ends_with(']') {
        let inner = &host[1..host.len() - 1];
        if let Ok(ip) = inner.parse::<Ipv6Addr>() && is_restricted_ip(&IpAddr::V6(ip)) {
            return Err(format!("Access to restricted IPv6 address {} is not allowed", ip));
        }
    }

    Ok(())
}

/// Check if an IP address is in a restricted range.
fn is_restricted_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // Loopback: 127.0.0.0/8
            if v4.is_loopback() {
                return true;
            }

            // Private ranges
            if v4.is_private() {
                return true;
            }

            // Link-local: 169.254.0.0/16 (includes AWS metadata 169.254.169.254)
            if v4.is_link_local() {
                return true;
            }

            // Broadcast: 255.255.255.255
            if v4.is_broadcast() {
                return true;
            }

            // Documentation: 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
            if v4.is_documentation() {
                return true;
            }

            // Shared address space: 100.64.0.0/10 (CGNAT)
            if matches!(v4.octets(), [100, 64..=127, ..]) {
                return true;
            }

            // IETF Protocol Assignments: 192.0.0.0/24
            if matches!(v4.octets(), [192, 0, 0, _]) {
                return true;
            }

            // Benchmark testing: 198.18.0.0/15
            if matches!(v4.octets(), [198, 18..=19, ..]) {
                return true;
            }

            // Multicast: 224.0.0.0/4
            if v4.is_multicast() {
                return true;
            }

            // Reserved for future use: 240.0.0.0/4
            if matches!(v4.octets(), [240..=255, ..]) {
                return true;
            }

            false
        }
        IpAddr::V6(v6) => {
            // Loopback: ::1
            if v6.is_loopback() {
                return true;
            }

            // Unique local (like private): fc00::/7
            if matches!(v6.segments(), [0xfc00..=0xfdff, ..]) {
                return true;
            }

            // Link-local: fe80::/10
            if matches!(v6.segments(), [0xfe80..=0xfebf, ..]) {
                return true;
            }

            // Multicast: ff00::/8
            if v6.is_multicast() {
                return true;
            }

            // Documentation: 2001:db8::/32
            if matches!(v6.segments(), [0x2001, 0x0db8, ..]) {
                return true;
            }

            false
        }
    }
}

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

        // Validate URL to prevent SSRF attacks
        if let Err(e) = validate_url(url) {
            return Ok(ToolResult {
                success: false,
                result: json!(null),
                error: Some(format!("URL validation failed: {}", e)),
                error_category: Some(ToolErrorCategory::Config),
                retryable: Some(false),
                retry_after_ms: None,
            });
        }

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
    use super::*;

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
