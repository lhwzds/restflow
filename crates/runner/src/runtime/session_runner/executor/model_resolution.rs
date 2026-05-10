use super::*;
use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::{OnceCell, RwLock};
use types::provider_meta;

impl AgentRuntimeExecutor {
    pub(super) fn build_llm_factory(
        api_keys: HashMap<LlmProvider, String>,
        model_specs: Vec<types::ModelSpec>,
    ) -> Arc<dyn LlmClientFactory> {
        #[cfg(any(test, feature = "test-utils"))]
        if let Some(factory) = current_test_llm_factory() {
            return factory;
        }

        Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs))
    }

    pub(super) fn should_skip_api_key_resolution() -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        {
            return current_test_llm_factory().is_some();
        }

        #[allow(unreachable_code)]
        false
    }

    pub(super) async fn resolve_api_key(
        &self,
        provider: Provider,
        agent_api_key_config: Option<&ApiKeyConfig>,
    ) -> Result<String> {
        // First, check agent-level API key config
        if let Some(config) = agent_api_key_config {
            match config {
                ApiKeyConfig::Direct(key) => {
                    if !key.is_empty() {
                        return Ok(key.clone());
                    }
                }
                ApiKeyConfig::Secret(secret_name) => {
                    if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
                        return Ok(secret_value);
                    }
                    return Err(anyhow!("Secret '{}' not found", secret_name));
                }
            }
        }

        if let Some(profile) = self.auth_manager.get_credential_for_model(provider).await {
            info!(
                profile_name = %profile.name,
                auth_provider = %profile.provider,
                model_provider = ?provider,
                "Using auth profile for model provider"
            );
            return profile.get_api_key(self.auth_manager.resolver());
        }

        // Fall back to well-known secret names for each provider
        let Some(secret_name) = provider.api_key_env() else {
            return Err(anyhow!(
                "No API key fallback is defined for provider {:?}. Please configure a compatible auth profile.",
                provider
            ));
        };

        for secret_name in provider.api_key_env_candidates() {
            if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
                return Ok(secret_value);
            }
        }

        Err(anyhow!(
            "No API key configured for provider {:?}. Please add secret '{}' in Settings.",
            provider,
            secret_name
        ))
    }

    /// Resolve API key, avoiding mismatched agent-level keys for fallback providers.
    pub(super) async fn resolve_api_key_for_model(
        &self,
        provider: Provider,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> Result<String> {
        let config = if provider == primary_provider {
            agent_api_key_config
        } else {
            None
        };
        self.resolve_api_key(provider, config).await
    }

    pub(super) fn context_window_for_model(model: ModelId) -> usize {
        match model {
            ModelId::ClaudeOpus4_6
            | ModelId::ClaudeSonnet4_5
            | ModelId::ClaudeHaiku4_5
            | ModelId::ClaudeCodeOpus
            | ModelId::ClaudeCodeSonnet
            | ModelId::ClaudeCodeHaiku => 200_000,
            ModelId::Gpt5
            | ModelId::Gpt5Mini
            | ModelId::Gpt5Nano
            | ModelId::Gpt5Pro
            | ModelId::Gpt5_1
            | ModelId::Gpt5_2
            | ModelId::Gpt5Codex
            | ModelId::Gpt5_1Codex
            | ModelId::Gpt5_2Codex
            | ModelId::CodexCli => 128_000,
            ModelId::Gpt5_4 | ModelId::Gpt5_4Codex => 1_000_000,
            ModelId::Gpt5_4Mini | ModelId::Gpt5_4Nano | ModelId::Gpt5_4MiniCodex => 400_000,
            ModelId::DeepseekChat | ModelId::DeepseekReasoner => 64_000,
            ModelId::Gemini25Pro
            | ModelId::Gemini25Flash
            | ModelId::Gemini3Pro
            | ModelId::Gemini3Flash
            | ModelId::GeminiCli => 1_000_000,
            _ => 128_000,
        }
    }

    pub(super) async fn resolve_model_from_stored_credentials(&self) -> Result<Option<ModelId>> {
        Ok(
            resolve_model_from_credentials(self.auth_manager.as_ref(), |key| {
                secret_exists(&self.storage.secrets, key)
            })
            .await,
        )
    }

    pub(super) async fn resolve_primary_model(&self, agent_node: &AgentNode) -> Result<ModelId> {
        if let Some(model_ref) = agent_node.resolved_model_ref() {
            return Ok(model_ref.model);
        }

        if let Some(model) = self.resolve_model_from_stored_credentials().await? {
            info!(
                selected_model = %model.as_str(),
                "Resolved model from stored credentials for agent without explicit model"
            );
            return Ok(model);
        }

        Err(anyhow!(
            "Model not specified. Please set a model for this agent or configure a compatible API secret/auth profile."
        ))
    }

    pub(super) async fn build_api_keys(
        &self,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> HashMap<LlmProvider, String> {
        let mut keys = HashMap::new();

        for provider in Provider::all().iter().copied() {
            if provider.api_key_env().is_none() {
                continue;
            }
            if let Ok(key) = self
                .resolve_api_key_for_model(provider, agent_api_key_config, primary_provider)
                .await
            {
                keys.insert(provider.as_llm_provider(), key);
            }
        }

        keys
    }

    pub(super) fn create_llm_client(
        factory: &dyn LlmClientFactory,
        model: ModelId,
        api_key: Option<&str>,
        agent_node: &AgentNode,
    ) -> Result<Arc<dyn LlmClient>> {
        if model.is_codex_cli() {
            let mut client = CodexClient::new().with_model(model.as_str());
            if let Some(effort) = agent_node
                .codex_cli_reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                client = client.with_reasoning_effort(effort);
            }
            if let Some(mode) = agent_node.codex_cli_execution_mode.as_ref() {
                client = client.with_execution_mode(mode.as_str());
            }
            return Ok(Arc::new(client));
        }

        Ok(factory.create_client(model.as_serialized_str(), api_key)?)
    }
}

const DEFAULT_MODELS_BASE_URL: &str = "https://models.dev";
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const OUTPUT_TOKENS_CAP: usize = 32_000;

#[derive(Debug, Clone, Copy)]
pub(super) struct ModelCapabilities {
    pub context_window: usize,
    pub input_limit: Option<usize>,
    pub output_limit: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ModelCatalogEntry {
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Default)]
struct CatalogState {
    by_provider_model: HashMap<String, ModelCatalogEntry>,
    by_model: HashMap<String, ModelCatalogEntry>,
    last_refresh: Option<Instant>,
}

pub(super) struct ModelCatalog {
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

    pub async fn resolve(&self, model: ModelId) -> Option<ModelCatalogEntry> {
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
}

#[derive(Debug, Deserialize)]
struct ModelsDevLimit {
    context: u64,
    input: Option<u64>,
    output: u64,
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

fn model_id_candidates(model: ModelId) -> Vec<String> {
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
    provider_meta(provider.as_model_provider()).models_dev_provider_ids
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
