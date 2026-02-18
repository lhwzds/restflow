//! Web fetch tool for reading static web pages
//!
//! Fetches a URL and extracts clean text content from HTML.
//! Works best with static content (news, blogs, docs, wikis).
//! For JavaScript-rendered pages (SPAs), use jina_reader instead.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv6Addr};

use crate::error::Result;
use crate::http_client::build_http_client;
use crate::tools::traits::{Tool, ToolOutput};

const MAX_CONTENT_LENGTH: usize = 12000;

#[derive(Debug, Deserialize)]
struct WebFetchInput {
    url: String,
}

/// Validate URL to prevent SSRF attacks.
/// Blocks access to internal/private network resources.
fn validate_url(url: &str) -> std::result::Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Only allow HTTP and HTTPS schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Scheme '{}' is not allowed. Only HTTP and HTTPS are permitted.", scheme)),
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
    if let Ok(ip) = host.parse::<IpAddr>()
        && is_restricted_ip(&ip)
    {
        return Err(format!(
            "Access to restricted IP address {} is not allowed (private/internal/metadata)",
            ip
        ));
    }

    // Handle bracketed IPv6 addresses
    if host.starts_with('[') && host.ends_with(']') {
        let inner = &host[1..host.len() - 1];
        if let Ok(ip) = inner.parse::<Ipv6Addr>()
            && is_restricted_ip(&IpAddr::V6(ip))
        {
            return Err(format!(
                "Access to restricted IPv6 address {} is not allowed",
                ip
            ));
        }
    }

    Ok(())
}

/// Check if an IP address is in a restricted range.
fn is_restricted_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_broadcast() || v4.is_documentation() {
                return true;
            }
            // CGNAT: 100.64.0.0/10
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
            // Reserved: 240.0.0.0/4
            if matches!(v4.octets(), [240..=255, ..]) {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_multicast() {
                return true;
            }
            // Unique local: fc00::/7
            if matches!(v6.segments(), [0xfc00..=0xfdff, ..]) {
                return true;
            }
            // Link-local: fe80::/10
            if matches!(v6.segments(), [0xfe80..=0xfebf, ..]) {
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

/// Web fetch tool that reads a webpage and returns clean text content.
///
/// Uses reqwest to fetch HTML and scraper to extract text,
/// stripping navigation, scripts, and styling.
pub struct WebFetchTool {
    client: Client,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
        }
    }
}

/// Extract clean text content from HTML, removing scripts, styles, nav, and other noise.
fn extract_text_from_html(html: &str) -> String {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let mut output = String::new();

    // Try to extract title
    if let Ok(title_sel) = Selector::parse("title")
        && let Some(title_el) = document.select(&title_sel).next()
    {
        let title: String = title_el.text().collect();
        let title = title.trim();
        if !title.is_empty() {
            output.push_str("# ");
            output.push_str(title);
            output.push_str("\n\n");
        }
    }

    // Selectors for noise elements to skip
    let noise_tags = [
        "script", "style", "nav", "footer", "header", "noscript", "svg", "iframe",
    ];
    let noise_selectors: Vec<_> = noise_tags
        .iter()
        .filter_map(|tag| Selector::parse(tag).ok())
        .collect();

    // Try to find article/main content first
    let content_selectors = ["article", "main", "[role=\"main\"]", ".content", "#content"];

    let content_root = content_selectors
        .iter()
        .filter_map(|sel| Selector::parse(sel).ok())
        .find_map(|sel| document.select(&sel).next());

    let root = content_root.unwrap_or_else(|| {
        // Fall back to body
        Selector::parse("body")
            .ok()
            .and_then(|sel| document.select(&sel).next())
            .unwrap_or_else(|| document.root_element())
    });

    // Collect text from root, skipping noise elements
    collect_text_recursive(&root, &noise_selectors, &mut output);

    // Clean up whitespace: collapse multiple newlines
    let mut cleaned = String::with_capacity(output.len());
    let mut prev_newline_count = 0;
    for ch in output.chars() {
        if ch == '\n' {
            prev_newline_count += 1;
            if prev_newline_count <= 2 {
                cleaned.push(ch);
            }
        } else {
            prev_newline_count = 0;
            cleaned.push(ch);
        }
    }

    cleaned.trim().to_string()
}

/// Recursively collect text content, skipping noise elements.
fn collect_text_recursive(
    element: &scraper::ElementRef,
    noise_selectors: &[scraper::Selector],
    output: &mut String,
) {
    use scraper::Node;

    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    output.push_str(trimmed);
                    output.push(' ');
                }
            }
            Node::Element(_) => {
                if let Some(child_ref) = scraper::ElementRef::wrap(child) {
                    // Skip noise elements
                    let is_noise = noise_selectors.iter().any(|sel| sel.matches(&child_ref));
                    if is_noise {
                        continue;
                    }

                    // Add newlines for block elements
                    let tag = child_ref.value().name();
                    let is_block = matches!(
                        tag,
                        "p" | "div"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                            | "li"
                            | "br"
                            | "tr"
                            | "blockquote"
                            | "pre"
                            | "section"
                    );

                    if is_block {
                        output.push('\n');
                    }

                    // Add heading markers
                    if tag.starts_with('h')
                        && tag.len() == 2
                        && let Some(level) = tag.chars().nth(1).and_then(|c| c.to_digit(10))
                    {
                        for _ in 0..level {
                            output.push('#');
                        }
                        output.push(' ');
                    }

                    collect_text_recursive(&child_ref, noise_selectors, output);

                    if is_block {
                        output.push('\n');
                    }
                }
            }
            _ => {}
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch and read a webpage. Returns clean text extracted from HTML. \
         Works best with static content (news articles, blog posts, documentation, wikis). \
         DO NOT use for: SPAs, React/Vue/Angular apps, pages with lazy-loaded content, or \
         any page that requires JavaScript to render. For those, use jina_reader instead. \
         If the result is empty or too short, the page likely requires JavaScript rendering."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch and read"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: WebFetchInput = serde_json::from_value(input)?;

        // Validate URL to prevent SSRF attacks
        if let Err(e) = validate_url(&params.url) {
            return Ok(ToolOutput::error(format!("URL validation failed: {}", e)));
        }

        let response = self
            .client
            .get(&params.url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            )
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Failed to fetch URL: {}. If the page requires JavaScript, try jina_reader instead. For connection errors, verify the URL is correct.",
                    e
                )));
            }
        };

        if !response.status().is_success() {
            return Ok(ToolOutput::error(format!(
                "HTTP {} when fetching {}",
                response.status(),
                params.url
            )));
        }

        let html = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Failed to read response body: {}",
                    e
                )));
            }
        };

        let mut content = extract_text_from_html(&html);

        // Truncate if too long
        if content.len() > MAX_CONTENT_LENGTH {
            content.truncate(MAX_CONTENT_LENGTH);
            content.push_str("\n\n[Content truncated]");
        }

        if content.is_empty() {
            Ok(ToolOutput::success(json!({
                "url": params.url,
                "content": "",
                "note": "No text content extracted. The page may use JavaScript rendering — try jina_reader instead."
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
    fn test_web_fetch_tool_schema() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "web_fetch");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert_eq!(schema["required"][0], "url");
    }

    #[test]
    fn test_extract_text_simple_html() {
        let html = "<html><body><p>Hello world</p></body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn test_extract_text_strips_scripts() {
        let html = r#"
        <html><body>
            <script>var x = 1;</script>
            <p>Real content here</p>
            <style>.foo { color: red; }</style>
        </body></html>
        "#;
        let text = extract_text_from_html(html);
        assert!(text.contains("Real content here"));
        assert!(!text.contains("var x = 1"));
        assert!(!text.contains("color: red"));
    }

    #[test]
    fn test_extract_text_with_article() {
        let html = r#"
        <html><body>
            <nav>Navigation stuff</nav>
            <article><h1>Article Title</h1><p>Article content goes here.</p></article>
            <footer>Footer stuff</footer>
        </body></html>
        "#;
        let text = extract_text_from_html(html);
        assert!(text.contains("Article Title"));
        assert!(text.contains("Article content goes here"));
        // Should prioritize article content
    }

    #[test]
    fn test_extract_text_with_title() {
        let html = "<html><head><title>Page Title</title></head><body><p>Content</p></body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("# Page Title"));
        assert!(text.contains("Content"));
    }

    #[test]
    fn test_extract_text_empty_html() {
        let html = "<html><body></body></html>";
        let text = extract_text_from_html(html);
        assert!(text.is_empty());
    }

    #[test]
    fn test_content_truncation() {
        let long_content = "A".repeat(MAX_CONTENT_LENGTH + 1000);
        let html = format!("<html><body><p>{}</p></body></html>", long_content);

        // Truncation happens in execute(), not extract_text_from_html()
        let text = extract_text_from_html(&html);
        assert!(text.len() > MAX_CONTENT_LENGTH);
    }

    #[test]
    fn test_url_validation_localhost_blocked() {
        assert!(validate_url("http://localhost/").is_err());
        assert!(validate_url("http://127.0.0.1/").is_err());
        assert!(validate_url("http://0.0.0.0/").is_err());
        assert!(validate_url("http://[::1]/").is_err());
    }

    #[test]
    fn test_url_validation_private_ip_blocked() {
        assert!(validate_url("http://10.0.0.1/").is_err());
        assert!(validate_url("http://172.16.0.1/").is_err());
        assert!(validate_url("http://192.168.1.1/").is_err());
    }

    #[test]
    fn test_url_validation_link_local_blocked() {
        assert!(validate_url("http://169.254.169.254/").is_err());
    }

    #[test]
    fn test_url_validation_public_ip_allowed() {
        assert!(validate_url("https://example.com/").is_ok());
        assert!(validate_url("https://api.github.com/").is_ok());
    }
}
