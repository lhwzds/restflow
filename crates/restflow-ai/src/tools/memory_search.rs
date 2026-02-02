use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::embedding::EmbeddingProvider;
use crate::error::{AiError, Result};
use crate::tools::traits::{Tool, ToolOutput};

pub struct MemorySearchTool {
    memory: Arc<dyn SemanticMemory>,
    embedding: Arc<dyn EmbeddingProvider>,
}

impl MemorySearchTool {
    pub fn new(memory: Arc<dyn SemanticMemory>, embedding: Arc<dyn EmbeddingProvider>) -> Self {
        Self { memory, embedding }
    }
}

#[derive(Debug, Clone)]
pub struct MemorySearchMatch {
    pub content: String,
    pub tags: Vec<String>,
    pub similarity: f32,
}

pub trait SemanticMemory: Send + Sync {
    fn semantic_search(
        &self,
        agent_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<MemorySearchMatch>>;
}

#[derive(Deserialize)]
struct MemorySearchInput {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    #[serde(default)]
    tags: Vec<String>,
}

fn default_top_k() -> usize {
    5
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant information using semantic similarity. \
         Use this to recall past conversations, stored knowledge, or task results."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language query to search for"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of results to return (default: 5)",
                    "default": 5
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional tags to filter results"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: MemorySearchInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        let query_embedding = self
            .embedding
            .embed(&params.query)
            .await
            .map_err(|e| AiError::Tool(e.to_string()))?;
        let agent_id = "default";
        let mut results = self
            .memory
            .semantic_search(agent_id, &query_embedding, params.top_k)
            .map_err(|e| AiError::Tool(e.to_string()))?;

        if !params.tags.is_empty() {
            results.retain(|m| params.tags.iter().all(|tag| m.tags.contains(tag)));
        }

        if results.is_empty() {
            return Ok(ToolOutput::success(json!("No relevant memories found.")));
        }

        let mut output = String::new();
        for (i, m) in results.iter().enumerate() {
            output.push_str(&format!(
                "\n{}. [Score: {:.2}]\n{}\n",
                i + 1,
                m.similarity,
                m.content
            ));
            if !m.tags.is_empty() {
                output.push_str(&format!(" Tags: {}\n", m.tags.join(", ")));
            }
        }

        Ok(ToolOutput::success(json!(output)))
    }
}
