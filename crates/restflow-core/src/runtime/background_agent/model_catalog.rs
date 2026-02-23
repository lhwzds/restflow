use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{OnceCell, RwLock};
use tracing::{debug, warn};

use crate::{AIModel, Provider};

const DEFAULT_MODELS_BASE_URL: &str = "https://models.dev";
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const OUTPUT_TOKENS_CAP: usize = 32_000;

#[derive(Debug, Clone, Copy)]
pub struct ModelCapabilities {
    pub context_window: usize,
    pub input_limit: Option<usize>,
    pub output_limit: usize,
}

pub use restflow_ai::llm::pricing::ModelPricing;

#[derive(Debug, Clone, Copy)]
pub struct ModelCatalogEntry {
    pub capabilities: ModelCapabilities,
    pub pricing: Option<ModelPricing>,
}

#[derive(Debug, Default)]
struct CatalogState {
    by_provider_model: HashMap<String, ModelCatalogEntry>,
    by_model: HashMap<String, ModelCatalogEntry>,
    last_refresh: Option<Instant>,
}

pub struct ModelCatalog {
    client: reqwest::Client,
    cache_path: PathBuf,
    state: RwLock<CatalogState>,
}

static GLOBAL_MODEL_CATALOG: OnceCell<Arc<ModelCatalog>> = OnceCell::const_new();

impl ModelCatalog {
    pub async fn global() -> Arc<Self> {
        GLOBAL_MODEL_CATALOG
            .get_or_init(|| async {
                let client = reqwest::Client::builder()
                    .timeout(REQUEST_TIMEOUT)
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                let cache_path = resolve_cache_path().unwrap_or_else(|_| {
                    PathBuf::from(".restflow").join("cache").join("models.json")
                });

                let catalog = Arc::new(Self {
                    client,
                    cache_path,
                    state: RwLock::new(CatalogState::default()),
                });
                catalog.load_cache_if_present().await;
                catalog.refresh_if_stale(true).await;
                catalog
            })
            .await
            .clone()
    }

    pub async fn resolve(&self, model: AIModel) -> Option<ModelCatalogEntry> {
        self.refresh_if_stale(false).await;

        let state = self.state.read().await;
        let provider_ids = models_dev_provider_candidates(model.provider());
        let model_ids = model_id_candidates(model);

        for provider_id in provider_ids {
            for model_id in &model_ids {
                let key = provider_model_key(provider_id, model_id);
                if let Some(entry) = state.by_provider_model.get(&key) {
                    return Some(*entry);
                }
            }
        }

        for model_id in &model_ids {
            let key = normalize(model_id);
            if let Some(entry) = state.by_model.get(&key) {
                return Some(*entry);
            }
        }

        None
    }

    async fn load_cache_if_present(&self) {
        let raw = match std::fs::read_to_string(&self.cache_path) {
            Ok(raw) => raw,
            Err(_) => return,
        };

        if let Ok(parsed) = parse_models_dev_json(&raw) {
            let mut state = self.state.write().await;
            state.by_provider_model = parsed.by_provider_model;
            state.by_model = parsed.by_model;
            state.last_refresh = Some(Instant::now());
        }
    }

    async fn refresh_if_stale(&self, force: bool) {
        if models_fetch_disabled() {
            return;
        }

        {
            let state = self.state.read().await;
            if !force
                && state
                    .last_refresh
                    .is_some_and(|last| last.elapsed() < DEFAULT_REFRESH_INTERVAL)
            {
                return;
            }
        }

        let mut state = self.state.write().await;
        if !force
            && state
                .last_refresh
                .is_some_and(|last| last.elapsed() < DEFAULT_REFRESH_INTERVAL)
        {
            return;
        }
        state.last_refresh = Some(Instant::now());
        drop(state);

        let url = models_url();
        match self.client.get(&url).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(raw) => match parse_models_dev_json(&raw) {
                    Ok(parsed) => {
                        {
                            let mut state = self.state.write().await;
                            state.by_provider_model = parsed.by_provider_model;
                            state.by_model = parsed.by_model;
                        }
                        if let Err(err) = write_cache(&self.cache_path, &raw) {
                            warn!(error = %err, "Failed to persist models.dev cache");
                        }
                        debug!("Refreshed models.dev catalog");
                    }
                    Err(err) => {
                        warn!(error = %err, "Failed to parse models.dev payload");
                        self.state.write().await.last_refresh = None;
                    }
                },
                Err(err) => {
                    warn!(error = %err, "Failed to read models.dev response body");
                    self.state.write().await.last_refresh = None;
                }
            },
            Ok(response) => {
                warn!(
                    status = response.status().as_u16(),
                    "models.dev returned non-success status"
                );
                self.state.write().await.last_refresh = None;
            }
            Err(err) => {
                debug!(error = %err, "Skipping models.dev refresh due to request error");
                self.state.write().await.last_refresh = None;
            }
        }
    }
}

#[derive(Debug, Default)]
struct ParsedCatalog {
    by_provider_model: HashMap<String, ModelCatalogEntry>,
    by_model: HashMap<String, ModelCatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevProvider {
    models: HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    id: Option<String>,
    limit: ModelsDevLimit,
    cost: Option<ModelsDevCost>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevLimit {
    context: u64,
    input: Option<u64>,
    output: u64,
}

#[derive(Debug, Deserialize)]
struct ModelsDevCost {
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
}

fn parse_models_dev_json(raw: &str) -> Result<ParsedCatalog> {
    let root: HashMap<String, ModelsDevProvider> =
        serde_json::from_str(raw).context("Failed to deserialize models.dev JSON")?;

    let mut parsed = ParsedCatalog::default();
    for (provider_id, provider) in root {
        for (model_key, model) in provider.models {
            let context_window = model.limit.context as usize;
            if context_window == 0 {
                continue;
            }

            let output_limit = if model.limit.output == 0 {
                OUTPUT_TOKENS_CAP
            } else {
                (model.limit.output as usize).min(OUTPUT_TOKENS_CAP)
            };

            let entry = ModelCatalogEntry {
                capabilities: ModelCapabilities {
                    context_window,
                    input_limit: model.limit.input.map(|v| v as usize),
                    output_limit,
                },
                pricing: model.cost.map(|cost| ModelPricing {
                    cost_per_1m_input: cost.input,
                    cost_per_1m_output: cost.output,
                    cache_read_per_1m: cost.cache_read,
                    cache_write_per_1m: cost.cache_write,
                }),
            };

            insert_entry(&mut parsed, &provider_id, &model_key, entry);
            if let Some(model_id) = model.id.as_deref() {
                insert_entry(&mut parsed, &provider_id, model_id, entry);
            }
        }
    }

    Ok(parsed)
}

fn insert_entry(
    parsed: &mut ParsedCatalog,
    provider_id: &str,
    model_id: &str,
    entry: ModelCatalogEntry,
) {
    parsed
        .by_provider_model
        .insert(provider_model_key(provider_id, model_id), entry);
    parsed.by_model.entry(normalize(model_id)).or_insert(entry);
}

fn provider_model_key(provider_id: &str, model_id: &str) -> String {
    format!("{}::{}", normalize(provider_id), normalize(model_id))
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn model_id_candidates(model: AIModel) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |value: String| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let key = normalize(trimmed);
        if seen.insert(key) {
            candidates.push(trimmed.to_string());
        }
    };

    let raw_ids = [model.as_str(), model.as_serialized_str()];
    for id in raw_ids {
        push(id.to_string());
        push(id.replace('.', "-"));
        push(id.replace('-', "."));

        if let Some(base) = id.strip_suffix("-preview") {
            push(base.to_string());
        }
        if let Some((_, tail)) = id.split_once('/') {
            push(tail.to_string());
            push(tail.replace('.', "-"));
            push(tail.replace('-', "."));
        }
    }

    candidates
}

fn models_dev_provider_candidates(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::OpenAI => &["openai"],
        Provider::Anthropic => &["anthropic"],
        Provider::DeepSeek => &["deepseek"],
        Provider::Google => &["google"],
        Provider::Groq => &["groq"],
        Provider::OpenRouter => &["openrouter"],
        Provider::XAI => &["xai"],
        Provider::Qwen => &["alibaba-cn", "alibaba"],
        Provider::Zai => &["zai", "zhipuai"],
        Provider::ZaiCodingPlan => &["zai-coding-plan", "zhipuai-coding-plan"],
        Provider::Moonshot => &["moonshotai", "moonshotai-cn", "kimi-for-coding"],
        Provider::Doubao => &["doubao", "doubao-cn", "ark"],
        Provider::Yi => &["yi"],
        Provider::SiliconFlow => &["siliconflow", "siliconflow-cn"],
        Provider::MiniMax => &["minimax", "minimax-cn"],
        Provider::MiniMaxCodingPlan => &["minimax-coding-plan", "minimax-cn-coding-plan"],
    }
}

fn models_url() -> String {
    let configured = std::env::var("RESTFLOW_MODELS_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match configured {
        Some(url) if url.ends_with(".json") => url,
        Some(url) => format!("{}/api.json", url.trim_end_matches('/')),
        None => format!("{}/api.json", DEFAULT_MODELS_BASE_URL),
    }
}

fn models_fetch_disabled() -> bool {
    std::env::var("RESTFLOW_DISABLE_MODELS_FETCH")
        .ok()
        .map(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn resolve_cache_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("RESTFLOW_MODELS_PATH")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path));
    }

    let cache_dir = crate::paths::ensure_restflow_dir()?.join("cache");
    std::fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir.join("models.json"))
}

fn write_cache(path: &PathBuf, raw: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, raw)?;
    Ok(())
}
