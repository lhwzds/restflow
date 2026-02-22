use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::Result;
use crate::tool::{Tool, ToolOutput};

/// Embedding provider trait for generating text embeddings.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
    fn model_name(&self) -> &str;
}

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
        "Search long-term memory using semantic similarity and optional tag filters. Returns matching memory chunks; use read_memory for direct id/tag/title retrieval."
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

        let query_embedding = match self.embedding.embed(&params.query).await {
            Ok(embedding) => embedding,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Failed to generate embedding: {e}. This requires a configured embedding provider. Check that OPENAI_API_KEY is set via manage_secrets."
                )));
            }
        };
        let agent_id = "default";
        let mut results = match self.memory.semantic_search(
            agent_id,
            &query_embedding,
            params.top_k,
        ) {
            Ok(results) => results,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Memory search failed: {e}. Try using list_memories with tag filters as an alternative."
                )));
            }
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use async_trait::async_trait;

    struct FailingEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for FailingEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Err(anyhow!("missing api key"))
        }

        async fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }

        fn dimension(&self) -> usize {
            1536
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }

    struct OkEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for OkEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }

        fn dimension(&self) -> usize {
            3
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }

    struct FailingMemory;

    impl SemanticMemory for FailingMemory {
        fn semantic_search(
            &self,
            _agent_id: &str,
            _query_embedding: &[f32],
            _top_k: usize,
        ) -> anyhow::Result<Vec<MemorySearchMatch>> {
            Err(anyhow!("index unavailable"))
        }
    }

    struct EmptyMemory;

    impl SemanticMemory for EmptyMemory {
        fn semantic_search(
            &self,
            _agent_id: &str,
            _query_embedding: &[f32],
            _top_k: usize,
        ) -> anyhow::Result<Vec<MemorySearchMatch>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_embedding_error_message() {
        let tool = MemorySearchTool::new(Arc::new(EmptyMemory), Arc::new(FailingEmbeddingProvider));

        let output = tool
            .execute(json!({"query": "test query"}))
            .await
            .expect("tool execution should not fail");

        assert!(!output.success);
        let error = output.error.expect("expected tool error");
        assert!(error.contains("Failed to generate embedding"));
        assert!(error.contains("OPENAI_API_KEY"));
        assert!(error.contains("manage_secrets"));
    }

    #[tokio::test]
    async fn test_search_error_message() {
        let tool = MemorySearchTool::new(Arc::new(FailingMemory), Arc::new(OkEmbeddingProvider));

        let output = tool
            .execute(json!({"query": "test query"}))
            .await
            .expect("tool execution should not fail");

        assert!(!output.success);
        let error = output.error.expect("expected tool error");
        assert!(error.contains("Memory search failed"));
        assert!(error.contains("list_memories"));
    }
}
