use super::provider::{EmbeddingConfig, EmbeddingProvider};
use anyhow::Result;
use async_trait::async_trait;

pub struct VoyageEmbedding {
    config: EmbeddingConfig,
}

impl VoyageEmbedding {
    pub fn new(model: String, dimension: usize) -> Self {
        Self {
            config: EmbeddingConfig {
                model,
                dimension,
                batch_size: 100,
                timeout_secs: 30,
            },
        }
    }
}

#[async_trait]
impl EmbeddingProvider for VoyageEmbedding {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        anyhow::bail!("Voyage embedding provider is not implemented")
    }

    async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        anyhow::bail!("Voyage embedding provider is not implemented")
    }

    fn dimension(&self) -> usize {
        self.config.dimension
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }
}
