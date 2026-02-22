//! Show Panel tool — displays rich content in the Tauri frontend Canvas panel.
//!
//! This is a Tauri-only tool: the frontend intercepts `tool_call_end` events
//! where `tool_name == "show_panel"` and renders the result in the Canvas.

use super::ToolResult;
use async_trait::async_trait;
use restflow_tools::error::{Result, ToolError};
use restflow_ai::tools::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct ShowPanelInput {
    /// Optional title for the panel header.
    title: Option<String>,
    /// Content to display.
    content: String,
    /// Content type hint for the frontend renderer.
    #[serde(default = "default_content_type")]
    content_type: String,
}

fn default_content_type() -> String {
    "markdown".to_string()
}

const VALID_CONTENT_TYPES: &[&str] = &["markdown", "code", "json", "html"];

pub struct ShowPanelTool;

impl Default for ShowPanelTool {
    fn default() -> Self {
        Self
    }
}

impl ShowPanelTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShowPanelTool {
    fn name(&self) -> &str {
        "show_panel"
    }

    fn description(&self) -> &str {
        "Display rich content (markdown, code, JSON, HTML) in the user's Canvas panel. \
         Use this to show formatted documents, code snippets, data visualizations, \
         skill content, agent configurations, or any structured information that \
         benefits from a dedicated display area separate from the chat."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Title displayed in the panel header"
                },
                "content": {
                    "type": "string",
                    "description": "The content to display (markdown, code, JSON string, or HTML)"
                },
                "content_type": {
                    "type": "string",
                    "enum": ["markdown", "code", "json", "html"],
                    "default": "markdown",
                    "description": "Content format hint for the renderer"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let parsed: ShowPanelInput = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid show_panel input: {e}")))?;

        if parsed.content.trim().is_empty() {
            return Ok(ToolResult::error("Content cannot be empty"));
        }

        if !VALID_CONTENT_TYPES.contains(&parsed.content_type.as_str()) {
            return Ok(ToolResult::error(format!(
                "Invalid content_type '{}'. Must be one of: {}",
                parsed.content_type,
                VALID_CONTENT_TYPES.join(", ")
            )));
        }

        let title = parsed.title.unwrap_or_default();

        Ok(ToolResult::success(json!({
            "displayed": true,
            "title": title,
            "content": parsed.content,
            "content_type": parsed.content_type,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_show_panel_markdown() {
        let tool = ShowPanelTool::new();
        let result = tool
            .execute(json!({
                "title": "My Skills",
                "content": "# Skills\n- skill-1\n- skill-2",
                "content_type": "markdown"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.result["displayed"], true);
        assert_eq!(result.result["title"], "My Skills");
        assert_eq!(result.result["content_type"], "markdown");
    }

    #[tokio::test]
    async fn test_show_panel_defaults_to_markdown() {
        let tool = ShowPanelTool::new();
        let result = tool
            .execute(json!({"content": "hello world"}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.result["content_type"], "markdown");
        assert_eq!(result.result["title"], "");
    }

    #[tokio::test]
    async fn test_show_panel_rejects_empty_content() {
        let tool = ShowPanelTool::new();
        let result = tool.execute(json!({"content": "  "})).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_show_panel_rejects_invalid_content_type() {
        let tool = ShowPanelTool::new();
        let result = tool
            .execute(json!({"content": "hello", "content_type": "xml"}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Invalid content_type"));
    }

    #[test]
    fn test_show_panel_schema() {
        let tool = ShowPanelTool::new();
        let schema = tool.schema();
        assert_eq!(schema.name, "show_panel");
        assert!(schema.parameters["properties"]["content"].is_object());
    }
}
