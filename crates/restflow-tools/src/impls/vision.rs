//! Vision tool for describing images via OpenAI chat completions.

use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs;

use crate::Result;
use crate::http_client::build_http_client;
use crate::{SecretResolver, Tool, ToolError, ToolOutput};

#[derive(Debug, Deserialize)]
struct VisionInput {
    file_path: String,
    prompt: Option<String>,
    model: Option<String>,
}

/// Tool for describing images using OpenAI vision models.
pub struct VisionTool {
    client: Client,
    secret_resolver: SecretResolver,
}

impl VisionTool {
    pub fn new(secret_resolver: SecretResolver) -> Self {
        Self {
            client: build_http_client(),
            secret_resolver,
        }
    }

    fn resolve_api_key(&self) -> Option<String> {
        (self.secret_resolver)("OPENAI_API_KEY")
    }

    fn detect_mime_type(path: &str) -> Option<&'static str> {
        match std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .as_deref()
        {
            Some("png") => Some("image/png"),
            Some("jpg") | Some("jpeg") => Some("image/jpeg"),
            Some("webp") => Some("image/webp"),
            Some("gif") => Some("image/gif"),
            _ => None,
        }
    }

    fn format_api_error(status: StatusCode, error_text: &str) -> String {
        match status {
            StatusCode::UNAUTHORIZED => {
                "Invalid API key. Check OPENAI_API_KEY in manage_secrets.".to_string()
            }
            StatusCode::TOO_MANY_REQUESTS => "Rate limited, retry later.".to_string(),
            _ => {
                if error_text.trim().is_empty() {
                    format!("Vision API returned HTTP {}.", status)
                } else {
                    error_text.to_string()
                }
            }
        }
    }
}

#[async_trait]
impl Tool for VisionTool {
    fn name(&self) -> &str {
        "vision"
    }

    fn description(&self) -> &str {
        "Analyze a local image and return a text description using OpenAI vision models."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Local path to an image file (png, jpg, jpeg, webp, gif)."
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional prompt for how to describe the image."
                },
                "model": {
                    "type": "string",
                    "description": "Optional model name. Defaults to gpt-4o."
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: VisionInput = serde_json::from_value(input)?;

        let api_key = self
            .resolve_api_key()
            .ok_or_else(|| {
                ToolError::Tool(
                    "Missing OPENAI_API_KEY. Set it via manage_secrets tool with {operation: 'set', key: 'OPENAI_API_KEY', value: '...'}.".to_string(),
                )
            })?;

        let mime_type = Self::detect_mime_type(&params.file_path).ok_or_else(|| {
            ToolError::Tool(format!(
                "Unsupported image type for '{}'. Supported: png, jpg, jpeg, webp, gif.",
                params.file_path
            ))
        })?;

        let image_bytes = fs::read(&params.file_path).await.map_err(|e| {
            ToolError::Tool(format!(
                "Cannot read image '{}': {}. Verify the file exists and is accessible.",
                params.file_path, e
            ))
        })?;

        let encoded = base64::engine::general_purpose::STANDARD.encode(image_bytes);
        let data_url = format!("data:{};base64,{}", mime_type, encoded);

        let prompt = params
            .prompt
            .clone()
            .unwrap_or_else(|| "Describe this image in detail.".to_string());
        let model = params.model.clone().unwrap_or_else(|| "gpt-4o".to_string());

        let payload = json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {"type": "image_url", "image_url": {"url": data_url}}
                    ]
                }
            ]
        });

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ToolError::Tool(format!(
                    "Vision API request failed: {}. This may be a network issue or rate limit. Retry after a brief wait.",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Ok(ToolOutput::error(format!(
                "Vision API request failed: {}",
                Self::format_api_error(status, &error_text)
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|_| {
                ToolError::Tool(
                    "Vision API returned an unexpected response format. This may indicate an API version mismatch. Retry or report the issue.".to_string(),
                )
            })?;

        let description = body
            .pointer("/choices/0/message/content")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();

        Ok(ToolOutput::success(json!({
            "description": description,
            "file_path": params.file_path,
            "model": model
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_vision_schema() {
        let resolver: SecretResolver = Arc::new(|_| None);
        let tool = VisionTool::new(resolver);
        let schema = tool.parameters_schema();
        assert_eq!(tool.name(), "vision");
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_vision_api_error_mapping() {
        let unauthorized = VisionTool::format_api_error(StatusCode::UNAUTHORIZED, "ignored");
        assert!(unauthorized.contains("Invalid API key"));

        let rate_limited = VisionTool::format_api_error(StatusCode::TOO_MANY_REQUESTS, "ignored");
        assert!(rate_limited.contains("Rate limited"));

        let passthrough = VisionTool::format_api_error(StatusCode::BAD_REQUEST, "custom message");
        assert_eq!(passthrough, "custom message");
    }
}
