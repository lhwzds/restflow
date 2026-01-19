//! Math tools for AI agents using rig-core

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};

/// Tool to add two numbers together
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddTool;

/// Input for the add operation
#[derive(Deserialize, Serialize)]
pub struct AddInput {
    pub a: f64,
    pub b: f64,
}

/// Error type for math operations (infallible)
#[derive(Debug, thiserror::Error)]
#[error("Math error")]
pub struct MathError;

impl Tool for AddTool {
    const NAME: &'static str = "add";

    type Args = AddInput;
    type Output = f64;
    type Error = MathError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "add".to_string(),
            description: "Adds two numbers together".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "a": {
                        "type": "number",
                        "description": "First number to add"
                    },
                    "b": {
                        "type": "number",
                        "description": "Second number to add"
                    }
                },
                "required": ["a", "b"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(args.a + args.b)
    }
}
