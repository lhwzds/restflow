use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub model: String,
    pub dimension: usize,
    pub batch_size: usize,
    pub timeout_secs: u64,
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for a batch of texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Get embedding dimension.
    fn dimension(&self) -> usize;

    /// Get model name.
    fn model_name(&self) -> &str;

    /// Normalize text before embedding (optional).
    fn normalize_text(&self, text: &str) -> String {
        text.trim()
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}
