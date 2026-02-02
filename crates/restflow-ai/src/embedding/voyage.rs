use super::provider::{EmbeddingConfig, EmbeddingProvider};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct VoyageEmbedding {
    client: Client,
    api_key: String,
    config: EmbeddingConfig,
}

impl VoyageEmbedding {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "voyage-3".to_string());
        let dimension = match model.as_str() {
            "voyage-3" => 1024,
            "voyage-3-large" => 2048,
            _ => 1024,
        };

        Self {
            client: Client::new(),
            api_key,
            config: EmbeddingConfig {
                model,
                dimension,
                batch_size: 100,
                timeout_secs: 30,
            },
        }
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[async_trait]
impl EmbeddingProvider for VoyageEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let normalized = self.normalize_text(text);
        let embeddings = self.embed_batch(&[normalized]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            model: self.config.model.clone(),
            input: texts.to_vec(),
        };

        let response = self
            .client
            .post("https://api.voyageai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            anyhow::bail!("Voyage API error {}: {}", status, error_text);
        }

        let data: EmbeddingResponse = response.json().await?;
        let mut sorted: Vec<_> = data.data.into_iter().collect();
        sorted.sort_by_key(|d| d.index);
        Ok(sorted.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.config.dimension
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }
}
