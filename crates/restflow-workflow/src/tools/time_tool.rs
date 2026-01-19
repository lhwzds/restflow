//! Time tool for AI agents using rig-core

use chrono::Utc;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

/// Tool to get the current time
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTimeTool;

/// Input for the get_current_time operation (empty)
#[derive(Deserialize, Serialize)]
pub struct GetTimeInput {}

/// Error type for time operations (infallible)
#[derive(Debug, thiserror::Error)]
#[error("Time error")]
pub struct TimeError;

impl Tool for GetTimeTool {
    const NAME: &'static str = "get_current_time";

    type Args = GetTimeInput;
    type Output = String;
    type Error = TimeError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "get_current_time".to_string(),
            description: "Gets the current UTC time".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(Utc::now().to_rfc3339())
    }
}
