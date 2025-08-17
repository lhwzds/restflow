use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
pub struct TimeArgs {
    // Empty struct - no arguments needed
}

#[derive(Debug, thiserror::Error)]
#[error("Time error")]
pub struct TimeError;

#[derive(Deserialize, Serialize)]
pub struct GetTimeTool;

impl Tool for GetTimeTool {
    const NAME: &'static str = "get_current_time";
    type Error = TimeError;
    type Args = TimeArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get the current date and time in local timezone".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let now: DateTime<Local> = Local::now();
        let time_str = now.to_rfc3339();
        
        println!("🕐 GetTimeTool called: {}", time_str);
        Ok(time_str)
    }
}