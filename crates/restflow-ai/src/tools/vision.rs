//! Vision tool for analyzing images using OpenAI.

use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::tools::traits::{SecretResolver, Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct VisionInput {
    file_path: String,
    prompt: Option<String>,
    model: Option<String>,
}

/// Tool for describing images via OpenAI vision models.
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

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    fn detect_mime(path: &Path) -> &'static str {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            Some("bmp") => "image/bmp",
            Some("tif") | Some("tiff") => "image/tiff",
            _ => "application/octet-stream",
        }
    }
}

#[async_trait]
impl Tool for VisionTool {
    fn name(&self) -> &str {
        "vision"
    }

    fn description(&self) -> &str {
        "Analyze images and return a detailed description."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the image file on disk"
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional prompt to guide the description"
                },
                "model": {
                    "type": "string",
                    "description": "Optional vision model (default: gpt-4o)"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: VisionInput = serde_json::from_value(input)?;
        let Some(api_key) = (self.secret_resolver)("OPENAI_API_KEY") else {
            return Ok(ToolOutput::error("Missing OPENAI_API_KEY"));
        };

        let model = params.model.unwrap_or_else(|| "gpt-4o".to_string());
        let prompt = params
            .prompt
            .unwrap_or_else(|| "Describe this image in detail.".to_string());
        let file_bytes = tokio::fs::read(&params.file_path).await?;

        let mime_type = Self::detect_mime(Path::new(&params.file_path));
        let encoded = base64::engine::general_purpose::STANDARD.encode(file_bytes);
        let data_url = format!("data:{};base64,{}", mime_type, encoded);

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
            .await;

        let response = match response {
            Ok(response) => response,
            Err(err) => return Ok(ToolOutput::error(err.to_string())),
        };

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(ToolOutput::error(format!(
                "OpenAI API error ({}): {}",
                status.as_u16(),
                body
            )));
        }

        let parsed: Value = serde_json::from_str(&body).map_err(AiError::from)?;
        let description = parsed
            .get("choices")
            .and_then(|value| value.get(0))
            .and_then(|value| value.get("message"))
            .and_then(|value| value.get("content"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();

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
        let tool = VisionTool::new(Arc::new(|_| None));
        assert_eq!(tool.name(), "vision");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_detect_mime() {
        assert_eq!(VisionTool::detect_mime(Path::new("image.png")), "image/png");
        assert_eq!(
            VisionTool::detect_mime(Path::new("image.jpg")),
            "image/jpeg"
        );
        assert_eq!(
            VisionTool::detect_mime(Path::new("image.unknown")),
            "application/octet-stream"
        );
    }

    #[tokio::test]
    async fn test_vision_requires_api_key() {
        let tool = VisionTool::new(Arc::new(|_| None));
        let output = tool
            .execute(json!({
                "file_path": "/tmp/does-not-exist.png"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.unwrap_or_default().contains("OPENAI_API_KEY"));
    }
}
