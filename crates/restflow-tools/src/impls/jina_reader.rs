//! Jina Reader tool for reading JavaScript-rendered web pages
//!
//! Proxies URLs through Jina Reader API (https://r.jina.ai/) to get
//! rendered page content as clean Markdown. Handles SPAs, React/Vue apps,
//! and other JS-heavy pages that web_fetch cannot process.

use async_trait::async_trait;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Result;
use crate::http_client::build_http_client;
use crate::tool::{Tool, ToolOutput};

const MAX_CONTENT_LENGTH: usize = 12000;

#[derive(Debug, Deserialize)]
struct JinaReaderInput {
    url: String,
}

/// Jina Reader tool that reads web pages via the Jina Reader cloud service.
///
/// Handles JavaScript-rendered pages (SPAs, React/Vue apps) by using
/// headless browser rendering on the server side. Returns content as Markdown.
pub struct JinaReaderTool {
    client: Client,
}

impl Default for JinaReaderTool {
    fn default() -> Self {
        Self::new()
    }
}

impl JinaReaderTool {
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
        }
    }

    fn format_http_error(status: StatusCode, url: &str) -> String {
        match status {
            StatusCode::TOO_MANY_REQUESTS => {
                "Jina Reader rate limited. Wait a moment and retry, or use web_fetch instead."
                    .to_string()
            }
            StatusCode::FORBIDDEN => {
                "Jina Reader blocked for this URL. Try web_fetch instead.".to_string()
            }
            _ => format!(
                "Jina Reader returned HTTP {} for {}. Try web_fetch as an alternative.",
                status, url
            ),
        }
    }
}

#[async_trait]
impl Tool for JinaReaderTool {
    fn name(&self) -> &str {
        "jina_reader"
    }

    fn description(&self) -> &str {
        "Read a webpage using Jina Reader (cloud service). Handles JavaScript-rendered pages \
         (SPAs, React/Vue/Angular apps, dynamic content, lazy-loaded content). Use this when \
         web_fetch returns empty or insufficient content. Returns page content as clean Markdown. \
         DO NOT use for: static pages (news, blogs, docs, wikis) — use web_fetch instead (faster, \
         no external service, no API rate limits). This is an external service (r.jina.ai)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to read"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: JinaReaderInput = serde_json::from_value(input)?;

        let jina_url = format!("https://r.jina.ai/{}", params.url);
        let response = self
            .client
            .get(&jina_url)
            .header("Accept", "text/markdown")
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Jina Reader service unavailable: {}. Try web_fetch for static pages, or retry later.",
                    e
                )));
            }
        };

        if !response.status().is_success() {
            return Ok(ToolOutput::error(Self::format_http_error(
                response.status(),
                &params.url,
            )));
        }

        let mut content = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Failed to read Jina Reader response: {}. Try web_fetch instead.",
                    e
                )));
            }
        };

        // Truncate if too long
        if content.len() > MAX_CONTENT_LENGTH {
            content.truncate(MAX_CONTENT_LENGTH);
            content.push_str("\n\n[Content truncated]");
        }

        if content.trim().is_empty() {
            Ok(ToolOutput::success(json!({
                "url": params.url,
                "content": "",
                "note": "Jina Reader returned empty content for this URL."
            })))
        } else {
            Ok(ToolOutput::success(json!({
                "url": params.url,
                "content": content
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jina_reader_tool_schema() {
        let tool = JinaReaderTool::new();
        assert_eq!(tool.name(), "jina_reader");
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("JavaScript"));

        let schema = tool.parameters_schema();
        assert_eq!(schema["required"][0], "url");
    }

    #[test]
    fn test_jina_reader_http_error_mapping() {
        let rate_limit =
            JinaReaderTool::format_http_error(StatusCode::TOO_MANY_REQUESTS, "https://a.com");
        assert!(rate_limit.contains("rate limited"));

        let forbidden = JinaReaderTool::format_http_error(StatusCode::FORBIDDEN, "https://a.com");
        assert!(forbidden.contains("blocked"));

        let generic = JinaReaderTool::format_http_error(StatusCode::BAD_GATEWAY, "https://a.com");
        assert!(generic.contains("Try web_fetch as an alternative"));
    }
}
