//! Vision tool for describing images via OpenAI chat completions.

use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::tools::traits::{SecretResolver, Tool, ToolOutput};

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
            .ok_or_else(|| AiError::Tool("Missing OPENAI_API_KEY secret".to_string()))?;

        let mime_type = Self::detect_mime_type(&params.file_path)
            .ok_or_else(|| AiError::Tool("Unsupported image type".to_string()))?;

        let image_bytes = fs::read(&params.file_path)
            .await
            .map_err(|e| AiError::Tool(format!("Failed to read image file: {}", e)))?;

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
            .map_err(|e| AiError::Tool(format!("Vision request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Ok(ToolOutput::error(format!(
                "Vision request failed: {}",
                error_text
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AiError::Tool(format!("Failed to parse vision response: {}", e)))?;

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
}
