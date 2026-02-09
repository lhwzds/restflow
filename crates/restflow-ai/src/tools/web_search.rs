//! Web search tool for searching the internet
//!
//! Supports multiple search providers with auto-selection:
//! - Brave Search API (needs BRAVE_API_KEY)
//! - Tavily Search API (needs TAVILY_API_KEY)
//! - DuckDuckGo HTML (free, no API key, best-effort)

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Result;
use crate::http_client::build_http_client;
use crate::tools::traits::{SecretResolver, Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    num_results: Option<usize>,
}

/// Web search tool that searches the internet for information.
///
/// Auto-selects the best available provider:
/// Brave (if BRAVE_API_KEY set) -> Tavily (if TAVILY_API_KEY set) -> DuckDuckGo (free)
pub struct WebSearchTool {
    client: Client,
    secret_resolver: Option<SecretResolver>,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
            secret_resolver: None,
        }
    }

    pub fn with_secret_resolver(mut self, resolver: SecretResolver) -> Self {
        self.secret_resolver = Some(resolver);
        self
    }

    fn resolve_secret(&self, key: &str) -> Option<String> {
        self.secret_resolver.as_ref().and_then(|r| r(key))
    }

    async fn brave_search(&self, query: &str, num: usize, api_key: &str) -> Result<Value> {
        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            urlencoding::encode(query),
            num
        );
        let response = self
            .client
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::AiError::Llm(format!(
                "Brave Search API error ({}): {}",
                status, body
            )));
        }

        let data: Value = response.json().await?;
        let results = data["web"]["results"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(num)
                    .map(|r| {
                        json!({
                            "title": r["title"].as_str().unwrap_or(""),
                            "url": r["url"].as_str().unwrap_or(""),
                            "snippet": r["description"].as_str().unwrap_or("")
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(json!({ "provider": "brave", "results": results }))
    }

    async fn tavily_search(&self, query: &str, num: usize, api_key: &str) -> Result<Value> {
        let body = json!({
            "api_key": api_key,
            "query": query,
            "max_results": num
        });
        let response = self
            .client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::AiError::Llm(format!(
                "Tavily Search API error ({}): {}",
                status, body
            )));
        }

        let data: Value = response.json().await?;
        let results = data["results"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(num)
                    .map(|r| {
                        json!({
                            "title": r["title"].as_str().unwrap_or(""),
                            "url": r["url"].as_str().unwrap_or(""),
                            "snippet": r["content"].as_str().unwrap_or("")
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(json!({ "provider": "tavily", "results": results }))
    }

    async fn duckduckgo_search(&self, query: &str, num: usize) -> Result<Value> {
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );
        let response = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            )
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(crate::error::AiError::Llm(format!(
                "DuckDuckGo returned status {}",
                response.status()
            )));
        }

        let html = response.text().await?;
        let results = parse_duckduckgo_html(&html, num);
        Ok(json!({ "provider": "duckduckgo", "results": results }))
    }
}

/// Parse DuckDuckGo HTML lite results page
fn parse_duckduckgo_html(html: &str, max_results: usize) -> Vec<Value> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let mut results = Vec::new();

    // DuckDuckGo HTML lite uses .result class for each result
    let result_sel = Selector::parse(".result").unwrap();
    let link_sel = Selector::parse(".result__a").unwrap();
    let snippet_sel = Selector::parse(".result__snippet").unwrap();

    for element in document.select(&result_sel).take(max_results) {
        let title = element
            .select(&link_sel)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        let url = element
            .select(&link_sel)
            .next()
            .and_then(|el| el.value().attr("href"))
            .unwrap_or("")
            .to_string();
        let normalized_url = normalize_duckduckgo_url(&url);

        let snippet = element
            .select(&snippet_sel)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        if !title.is_empty() && !normalized_url.is_empty() {
            results.push(json!({
                "title": title,
                "url": normalized_url,
                "snippet": snippet
            }));
        }
    }

    results
}

/// Normalize DuckDuckGo tracking links to the destination URL.
///
/// DDG HTML results often return links like:
/// https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com
fn normalize_duckduckgo_url(raw_url: &str) -> String {
    let Ok(parsed) = url::Url::parse(raw_url) else {
        return raw_url.to_string();
    };

    if parsed.domain() == Some("duckduckgo.com") && parsed.path().starts_with("/l/") {
        for (key, value) in parsed.query_pairs() {
            if key == "uddg" {
                return value.into_owned();
            }
        }
    }

    raw_url.to_string()
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns a list of results with titles, URLs, and snippets. \
         Use this to find information, news, documentation, or answers to questions. \
         After searching, use web_fetch or jina_reader to read specific pages."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results to return (default: 5, max: 10)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: WebSearchInput = serde_json::from_value(input)?;
        let num = params.num_results.unwrap_or(5).min(10);

        // Auto-select provider: Brave -> Tavily -> DuckDuckGo
        if let Some(key) = self.resolve_secret("BRAVE_API_KEY") {
            match self.brave_search(&params.query, num, &key).await {
                Ok(results) => return Ok(ToolOutput::success(results)),
                Err(e) => {
                    tracing::warn!(error = %e, "Brave Search failed, trying next provider");
                }
            }
        }

        if let Some(key) = self.resolve_secret("TAVILY_API_KEY") {
            match self.tavily_search(&params.query, num, &key).await {
                Ok(results) => return Ok(ToolOutput::success(results)),
                Err(e) => {
                    tracing::warn!(error = %e, "Tavily Search failed, trying next provider");
                }
            }
        }

        match self.duckduckgo_search(&params.query, num).await {
            Ok(results) => Ok(ToolOutput::success(results)),
            Err(e) => Ok(ToolOutput::error(format!(
                "All search providers failed. Last error: {}",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_schema() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert_eq!(schema["required"][0], "query");
    }

    #[test]
    fn test_parse_duckduckgo_html_empty() {
        let html = "<html><body></body></html>";
        let results = parse_duckduckgo_html(html, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_duckduckgo_html_with_results() {
        let html = r#"
        <html><body>
            <div class="result">
                <a class="result__a" href="https://example.com">Example Title</a>
                <a class="result__snippet">This is a snippet about example.</a>
            </div>
            <div class="result">
                <a class="result__a" href="https://test.com">Test Title</a>
                <a class="result__snippet">This is a test snippet.</a>
            </div>
        </body></html>
        "#;
        let results = parse_duckduckgo_html(html, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["title"], "Example Title");
        assert_eq!(results[0]["url"], "https://example.com");
        assert_eq!(results[0]["snippet"], "This is a snippet about example.");
    }

    #[test]
    fn test_parse_duckduckgo_html_respects_limit() {
        let html = r#"
        <html><body>
            <div class="result">
                <a class="result__a" href="https://a.com">A</a>
                <a class="result__snippet">Snippet A</a>
            </div>
            <div class="result">
                <a class="result__a" href="https://b.com">B</a>
                <a class="result__snippet">Snippet B</a>
            </div>
            <div class="result">
                <a class="result__a" href="https://c.com">C</a>
                <a class="result__snippet">Snippet C</a>
            </div>
        </body></html>
        "#;
        let results = parse_duckduckgo_html(html, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parse_duckduckgo_html_decodes_redirect_url() {
        let html = r#"
        <html><body>
            <div class="result">
                <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpost">Example</a>
                <a class="result__snippet">Snippet</a>
            </div>
        </body></html>
        "#;

        let results = parse_duckduckgo_html(html, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["url"], "https://example.com/post");
    }
}
