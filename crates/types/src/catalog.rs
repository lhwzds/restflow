use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{ClientKind, ModelId, ModelMetadata, ModelMetadataDTO, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub provider: Provider,
    pub api_name: &'static str,
    pub display_name: &'static str,
    pub supports_temperature: bool,
    pub aliases: &'static [&'static str],
    pub prefix_aliases: &'static [&'static str],
    pub canonical_family: Option<&'static str>,
    pub same_provider_fallback: Option<ModelId>,
    pub openrouter_equivalent: Option<ModelId>,
    pub client_kind: ClientKind,
    pub base_url_override: Option<&'static str>,
}

impl ModelDescriptor {
    pub const fn new(
        id: ModelId,
        provider: Provider,
        api_name: &'static str,
        display_name: &'static str,
        supports_temperature: bool,
    ) -> Self {
        Self {
            id,
            provider,
            api_name,
            display_name,
            supports_temperature,
            aliases: &[],
            prefix_aliases: &[],
            canonical_family: None,
            same_provider_fallback: None,
            openrouter_equivalent: None,
            client_kind: ClientKind::Http,
            base_url_override: None,
        }
    }

    pub const fn with_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
        self
    }

    pub const fn with_prefix_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.prefix_aliases = aliases;
        self
    }

    pub const fn with_canonical_family(mut self, family: &'static str) -> Self {
        self.canonical_family = Some(family);
        self
    }

    pub const fn with_same_provider_fallback(mut self, model: ModelId) -> Self {
        self.same_provider_fallback = Some(model);
        self
    }

    pub const fn with_openrouter_equivalent(mut self, model: ModelId) -> Self {
        self.openrouter_equivalent = Some(model);
        self
    }

    pub const fn with_client_kind(mut self, client_kind: ClientKind) -> Self {
        self.client_kind = client_kind;
        self
    }

    pub const fn with_base_url_override(mut self, base_url: &'static str) -> Self {
        self.base_url_override = Some(base_url);
        self
    }

    pub const fn metadata(&self) -> ModelMetadata {
        ModelMetadata {
            provider: self.provider,
            supports_temperature: self.supports_temperature,
            name: self.display_name,
        }
    }

    pub fn metadata_dto(&self) -> ModelMetadataDTO {
        ModelMetadataDTO {
            model: self.id,
            provider: self.provider,
            supports_temperature: self.supports_temperature,
            name: self.display_name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCatalog {
    pub provider: Provider,
    pub flagship: ModelId,
    pub models: &'static [ModelDescriptor],
}

impl ProviderCatalog {
    pub const fn new(
        provider: Provider,
        flagship: ModelId,
        models: &'static [ModelDescriptor],
    ) -> Self {
        Self {
            provider,
            flagship,
            models,
        }
    }
}

pub const PROVIDER_CATALOGS: &[ProviderCatalog] = &[
    openai::CATALOG,
    anthropic::CATALOG,
    codex::CATALOG,
    deepseek::CATALOG,
    google::CATALOG,
    groq::CATALOG,
    openrouter::CATALOG,
    xai::CATALOG,
    qwen::CATALOG,
    zai::CATALOG,
    zai_coding_plan::CATALOG,
    moonshot::CATALOG,
    doubao::CATALOG,
    yi::CATALOG,
    siliconflow::CATALOG,
    minimax::CATALOG,
    minimax_coding_plan::CATALOG,
];

static DESCRIPTOR_BY_ID: OnceLock<HashMap<&'static str, &'static ModelDescriptor>> =
    OnceLock::new();
static MODEL_IDS: OnceLock<Vec<ModelId>> = OnceLock::new();
static NAME_LOOKUP: OnceLock<HashMap<String, ModelId>> = OnceLock::new();

pub fn provider_catalog(provider: Provider) -> Option<&'static ProviderCatalog> {
    PROVIDER_CATALOGS
        .iter()
        .find(|catalog| catalog.provider == provider)
}

pub fn all_descriptors() -> impl Iterator<Item = &'static ModelDescriptor> {
    PROVIDER_CATALOGS
        .iter()
        .flat_map(|catalog| catalog.models.iter())
}

pub fn all_model_ids() -> &'static [ModelId] {
    MODEL_IDS
        .get_or_init(|| all_descriptors().map(|descriptor| descriptor.id).collect())
        .as_slice()
}

pub fn descriptor(model: ModelId) -> Option<&'static ModelDescriptor> {
    DESCRIPTOR_BY_ID
        .get_or_init(|| {
            all_descriptors()
                .map(|descriptor| (descriptor.id.as_serialized_str(), descriptor))
                .collect()
        })
        .get(model.as_serialized_str())
        .copied()
}

pub fn lookup_by_name(name: &str) -> Option<ModelId> {
    let key = normalize_lookup_key(name)?;
    if let Some(model) = NAME_LOOKUP
        .get_or_init(|| {
            let mut lookup = HashMap::new();
            for descriptor in all_descriptors() {
                register_lookup_key(
                    &mut lookup,
                    descriptor.id.as_serialized_str(),
                    descriptor.id,
                );
            }
            for descriptor in all_descriptors() {
                register_lookup_key(&mut lookup, descriptor.api_name, descriptor.id);
                for alias in descriptor.aliases {
                    register_lookup_key(&mut lookup, alias, descriptor.id);
                }
            }
            lookup
        })
        .get(&key)
        .copied()
    {
        return Some(model);
    }

    all_descriptors().find_map(|descriptor| {
        descriptor_matches_prefix_alias(descriptor, &key).then_some(descriptor.id)
    })
}

pub fn lookup_for_provider(provider: Provider, model: &str) -> Option<ModelId> {
    let key = normalize_lookup_key(model)?;
    provider_catalog(provider)?
        .models
        .iter()
        .find_map(|descriptor| {
            descriptor_matches_lookup_key(descriptor, &key).then_some(descriptor.id)
        })
}

pub fn lookup_by_canonical_family(provider: Provider, canonical_family: &str) -> Option<ModelId> {
    provider_catalog(provider)?
        .models
        .iter()
        .find_map(|descriptor| {
            (descriptor.canonical_family == Some(canonical_family)).then_some(descriptor.id)
        })
}

fn descriptor_matches_lookup_key(descriptor: &ModelDescriptor, key: &str) -> bool {
    descriptor.id.as_serialized_str().eq_ignore_ascii_case(key)
        || descriptor.api_name.eq_ignore_ascii_case(key)
        || descriptor
            .aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(key))
        || descriptor_matches_prefix_alias(descriptor, key)
}

fn descriptor_matches_prefix_alias(descriptor: &ModelDescriptor, key: &str) -> bool {
    descriptor
        .prefix_aliases
        .iter()
        .any(|alias| key.starts_with(alias))
}

fn register_lookup_key(lookup: &mut HashMap<String, ModelId>, raw: &str, model: ModelId) {
    if let Some(key) = normalize_lookup_key(raw) {
        lookup.entry(key).or_insert(model);
    }
}

fn normalize_lookup_key(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_ascii_lowercase())
    }
}

mod anthropic {
    use super::*;

    const OPUS_ALIASES: &[&str] = &["claude-opus-4-6-20260205", "claude-opus-4-6-20250514"];
    const SONNET_ALIASES: &[&str] = &["claude-sonnet-4-5-20250514", "claude-sonnet-4-20250514"];
    const HAIKU_ALIASES: &[&str] = &["claude-haiku-4-5-20250514", "claude-haiku-4-20250514"];
    const OPUS_PREFIX_ALIASES: &[&str] = &["claude-opus-4-6", "claude-opus-4"];
    const SONNET_PREFIX_ALIASES: &[&str] = &["claude-sonnet-4"];
    const HAIKU_PREFIX_ALIASES: &[&str] = &["claude-haiku-4"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::ClaudeOpus4_6,
            Provider::Anthropic,
            "claude-opus-4-6",
            "Claude Opus 4.6",
            true,
        )
        .with_aliases(OPUS_ALIASES)
        .with_prefix_aliases(OPUS_PREFIX_ALIASES)
        .with_same_provider_fallback(ModelId::ClaudeSonnet4_5)
        .with_openrouter_equivalent(ModelId::OrClaudeOpus4_6),
        ModelDescriptor::new(
            ModelId::ClaudeSonnet4_5,
            Provider::Anthropic,
            "claude-sonnet-4-5",
            "Claude Sonnet 4.5",
            true,
        )
        .with_aliases(SONNET_ALIASES)
        .with_prefix_aliases(SONNET_PREFIX_ALIASES)
        .with_same_provider_fallback(ModelId::ClaudeHaiku4_5)
        .with_openrouter_equivalent(ModelId::OrClaudeOpus4_6),
        ModelDescriptor::new(
            ModelId::ClaudeHaiku4_5,
            Provider::Anthropic,
            "claude-haiku-4-5",
            "Claude Haiku 4.5",
            true,
        )
        .with_aliases(HAIKU_ALIASES)
        .with_prefix_aliases(HAIKU_PREFIX_ALIASES),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Anthropic, ModelId::ClaudeSonnet4_5, MODELS);
}

mod codex {
    use super::*;

    const GPT_5_4_ALIASES: &[&str] = &["gpt-5.4-codex"];
    const GPT_5_4_MINI_ALIASES: &[&str] = &["gpt-5.4-mini-codex"];
    const GPT_5_5_ALIASES: &[&str] = &["gpt-5.5-codex"];
    const GPT_5_5_PRO_ALIASES: &[&str] = &["gpt-5.5-pro-codex"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::Gpt5_4Codex,
            Provider::Codex,
            "gpt-5.4",
            "GPT-5.4",
            false,
        )
        .with_aliases(GPT_5_4_ALIASES)
        .with_client_kind(ClientKind::CodexCli)
        .with_same_provider_fallback(ModelId::Gpt5_4MiniCodex),
        ModelDescriptor::new(
            ModelId::Gpt5_4MiniCodex,
            Provider::Codex,
            "gpt-5.4-mini",
            "GPT-5.4 Mini",
            false,
        )
        .with_aliases(GPT_5_4_MINI_ALIASES)
        .with_client_kind(ClientKind::CodexCli),
        ModelDescriptor::new(
            ModelId::Gpt5_5Codex,
            Provider::Codex,
            "gpt-5.5",
            "Codex GPT-5.5",
            false,
        )
        .with_aliases(GPT_5_5_ALIASES)
        .with_client_kind(ClientKind::CodexCli)
        .with_same_provider_fallback(ModelId::Gpt5_4Codex),
        ModelDescriptor::new(
            ModelId::Gpt5_5ProCodex,
            Provider::Codex,
            "gpt-5.5-pro",
            "Codex GPT-5.5 Pro",
            false,
        )
        .with_aliases(GPT_5_5_PRO_ALIASES)
        .with_client_kind(ClientKind::CodexCli)
        .with_same_provider_fallback(ModelId::Gpt5_5Codex),
        ModelDescriptor::new(
            ModelId::Gpt5Codex,
            Provider::Codex,
            "gpt-5-codex",
            "Codex GPT-5",
            false,
        )
        .with_client_kind(ClientKind::CodexCli),
        ModelDescriptor::new(
            ModelId::Gpt5_1Codex,
            Provider::Codex,
            "gpt-5.1-codex",
            "Codex GPT-5.1",
            false,
        )
        .with_client_kind(ClientKind::CodexCli),
        ModelDescriptor::new(
            ModelId::Gpt5_2Codex,
            Provider::Codex,
            "gpt-5.2-codex",
            "Codex GPT-5.2",
            false,
        )
        .with_client_kind(ClientKind::CodexCli),
        ModelDescriptor::new(
            ModelId::CodexCli,
            Provider::Codex,
            "gpt-5.3-codex",
            "Codex GPT-5.3",
            false,
        )
        .with_client_kind(ClientKind::CodexCli),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Codex, ModelId::Gpt5_5Codex, MODELS);
}

mod deepseek {
    use super::*;

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::DeepseekChat,
            Provider::DeepSeek,
            "deepseek-chat",
            "DeepSeek Chat",
            true,
        )
        .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
        ModelDescriptor::new(
            ModelId::DeepseekReasoner,
            Provider::DeepSeek,
            "deepseek-reasoner",
            "DeepSeek Reasoner",
            true,
        )
        .with_same_provider_fallback(ModelId::DeepseekChat)
        .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
        ModelDescriptor::new(
            ModelId::DeepseekV4Pro,
            Provider::DeepSeek,
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            true,
        )
        .with_same_provider_fallback(ModelId::DeepseekV4Flash)
        .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
        ModelDescriptor::new(
            ModelId::DeepseekV4Flash,
            Provider::DeepSeek,
            "deepseek-v4-flash",
            "DeepSeek V4 Flash",
            true,
        )
        .with_same_provider_fallback(ModelId::DeepseekChat)
        .with_openrouter_equivalent(ModelId::OrDeepseekV3_2),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::DeepSeek, ModelId::DeepseekV4Pro, MODELS);
}

mod doubao {
    use super::*;

    const DOUBAO_ALIASES: &[&str] = &["doubao-pro", "doubao"];

    pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
        ModelId::DoubaoPro,
        Provider::Doubao,
        "doubao-pro-256k",
        "Doubao Pro",
        true,
    )
    .with_aliases(DOUBAO_ALIASES)];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Doubao, ModelId::DoubaoPro, MODELS);
}

mod google {
    use super::*;

    const GEMINI_25_PRO_ALIASES: &[&str] = &["gemini-pro"];
    const GEMINI_25_FLASH_ALIASES: &[&str] = &["gemini-flash"];
    const GEMINI_3_PRO_ALIASES: &[&str] = &["gemini-3-pro"];
    const GEMINI_3_FLASH_ALIASES: &[&str] = &["gemini-3-flash"];
    const GEMINI_CLI_ALIASES: &[&str] = &["gemini-cli"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::Gemini25Pro,
            Provider::Google,
            "gemini-2.5-pro",
            "Gemini 2.5 Pro",
            true,
        )
        .with_aliases(GEMINI_25_PRO_ALIASES)
        .with_same_provider_fallback(ModelId::Gemini25Flash)
        .with_openrouter_equivalent(ModelId::OrGemini3Pro),
        ModelDescriptor::new(
            ModelId::Gemini25Flash,
            Provider::Google,
            "gemini-2.5-flash",
            "Gemini 2.5 Flash",
            true,
        )
        .with_aliases(GEMINI_25_FLASH_ALIASES)
        .with_same_provider_fallback(ModelId::Gemini3Flash)
        .with_openrouter_equivalent(ModelId::OrGemini3Pro),
        ModelDescriptor::new(
            ModelId::Gemini3Pro,
            Provider::Google,
            "gemini-3-pro-preview",
            "Gemini 3 Pro Preview",
            true,
        )
        .with_aliases(GEMINI_3_PRO_ALIASES),
        ModelDescriptor::new(
            ModelId::Gemini3Flash,
            Provider::Google,
            "gemini-3-flash-preview",
            "Gemini 3 Flash Preview",
            true,
        )
        .with_aliases(GEMINI_3_FLASH_ALIASES),
        ModelDescriptor::new(
            ModelId::GeminiCli,
            Provider::Google,
            "gemini-2.5-pro",
            "Gemini CLI",
            false,
        )
        .with_aliases(GEMINI_CLI_ALIASES)
        .with_client_kind(ClientKind::GeminiCli),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Google, ModelId::Gemini3Pro, MODELS);
}

mod groq {
    use super::*;

    const SCOUT_ALIASES: &[&str] = &["groq-scout", "llama-4-scout"];
    const MAVERICK_ALIASES: &[&str] = &["groq-maverick", "llama-4-maverick"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::GroqLlama4Scout,
            Provider::Groq,
            "meta-llama/llama-4-scout-17b-16e-instruct",
            "Llama 4 Scout",
            true,
        )
        .with_aliases(SCOUT_ALIASES),
        ModelDescriptor::new(
            ModelId::GroqLlama4Maverick,
            Provider::Groq,
            "meta-llama/llama-4-maverick-17b-128e-instruct",
            "Llama 4 Maverick",
            true,
        )
        .with_aliases(MAVERICK_ALIASES),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Groq, ModelId::GroqLlama4Maverick, MODELS);
}

mod minimax {
    use super::*;

    const M21_ALIASES: &[&str] = &["minimax-m2.1"];
    const M25_ALIASES: &[&str] = &["minimax-m2.5"];
    const M27_ALIASES: &[&str] = &["minimax-m2.7"];
    const M27_HIGHSPEED_ALIASES: &[&str] = &["minimax-m2.7-highspeed"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::MiniMaxM21,
            Provider::MiniMax,
            "MiniMax-M2.1",
            "MiniMax M2.1",
            true,
        )
        .with_aliases(M21_ALIASES)
        .with_canonical_family("minimax-m2-1")
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM25,
            Provider::MiniMax,
            "MiniMax-M2.5",
            "MiniMax M2.5",
            true,
        )
        .with_aliases(M25_ALIASES)
        .with_canonical_family("minimax-m2-5")
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM27,
            Provider::MiniMax,
            "MiniMax-M2.7",
            "MiniMax M2.7",
            true,
        )
        .with_aliases(M27_ALIASES)
        .with_canonical_family("minimax-m2-7")
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM27Highspeed,
            Provider::MiniMax,
            "MiniMax-M2.7-highspeed",
            "MiniMax M2.7 Highspeed",
            true,
        )
        .with_aliases(M27_HIGHSPEED_ALIASES)
        .with_canonical_family("minimax-m2-7-highspeed")
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::MiniMax, ModelId::MiniMaxM27, MODELS);
}

mod minimax_coding_plan {
    use super::*;

    const M21_ALIASES: &[&str] = &["minimax-coding-plan-m2.1", "minimax-m2-1", "minimax-m2.1"];
    const M25_ALIASES: &[&str] = &[
        "minimax-coding-plan-m2.5",
        "minimax-m2-5",
        "minimax-m2.5",
        "minimax-coding-plan",
        "minimax-coding",
        "coding-plan-minimax",
        "minimax/coding-plan",
    ];
    const M25_HIGHSPEED_ALIASES: &[&str] = &[
        "minimax-coding-plan-m2.5-highspeed",
        "minimax-m2-5-highspeed",
        "minimax-m2.5-highspeed",
    ];
    const M27_ALIASES: &[&str] = &["minimax-coding-plan-m2.7", "minimax-m2-7", "minimax-m2.7"];
    const M27_HIGHSPEED_ALIASES: &[&str] = &[
        "minimax-coding-plan-m2.7-highspeed",
        "minimax-m2-7-highspeed",
        "minimax-m2.7-highspeed",
    ];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::MiniMaxM21CodingPlan,
            Provider::MiniMaxCodingPlan,
            "MiniMax-M2.1",
            "MiniMax M2.1 (Coding Plan)",
            true,
        )
        .with_aliases(M21_ALIASES)
        .with_canonical_family("minimax-m2-1")
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM25CodingPlan,
            Provider::MiniMaxCodingPlan,
            "MiniMax-M2.5",
            "MiniMax M2.5 (Coding Plan)",
            true,
        )
        .with_aliases(M25_ALIASES)
        .with_canonical_family("minimax-m2-5")
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM25CodingPlanHighspeed,
            Provider::MiniMaxCodingPlan,
            "MiniMax-M2.5-highspeed",
            "MiniMax M2.5 Highspeed (Coding Plan)",
            true,
        )
        .with_aliases(M25_HIGHSPEED_ALIASES)
        .with_canonical_family("minimax-m2-5-highspeed")
        .with_same_provider_fallback(ModelId::MiniMaxM25CodingPlan)
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM27CodingPlan,
            Provider::MiniMaxCodingPlan,
            "MiniMax-M2.7",
            "MiniMax M2.7 (Coding Plan)",
            true,
        )
        .with_aliases(M27_ALIASES)
        .with_canonical_family("minimax-m2-7")
        .with_same_provider_fallback(ModelId::MiniMaxM25CodingPlan)
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
        ModelDescriptor::new(
            ModelId::MiniMaxM27CodingPlanHighspeed,
            Provider::MiniMaxCodingPlan,
            "MiniMax-M2.7-highspeed",
            "MiniMax M2.7 Highspeed (Coding Plan)",
            true,
        )
        .with_aliases(M27_HIGHSPEED_ALIASES)
        .with_canonical_family("minimax-m2-7-highspeed")
        .with_same_provider_fallback(ModelId::MiniMaxM27CodingPlan)
        .with_openrouter_equivalent(ModelId::OrMinimaxM2_1),
    ];

    // Keep the best-quality flagship distinct from the conservative default model.
    // Provider metadata owns the default selection (M2.5), while the catalog
    // flagship remains the recommended top-end coding-plan model (M2.7).
    pub const CATALOG: ProviderCatalog = ProviderCatalog::new(
        Provider::MiniMaxCodingPlan,
        ModelId::MiniMaxM27CodingPlan,
        MODELS,
    );

    #[cfg(test)]
    mod tests {
        use super::CATALOG;
        use crate::{ModelId, ModelProvider, provider_meta};

        #[test]
        fn default_model_is_intentionally_distinct_from_flagship() {
            let provider_meta = provider_meta(ModelProvider::MiniMaxCodingPlan);

            assert_eq!(
                provider_meta.default_model_id,
                ModelId::MiniMaxM25CodingPlan
            );
            assert_eq!(CATALOG.flagship, ModelId::MiniMaxM27CodingPlan);
            assert_ne!(provider_meta.default_model_id, CATALOG.flagship);
        }

        #[test]
        fn catalog_contains_default_and_flagship_models() {
            assert!(
                CATALOG
                    .models
                    .iter()
                    .any(|descriptor| descriptor.id == ModelId::MiniMaxM25CodingPlan)
            );
            assert!(
                CATALOG
                    .models
                    .iter()
                    .any(|descriptor| descriptor.id == ModelId::MiniMaxM27CodingPlan)
            );
        }
    }
}

mod moonshot {
    use super::*;

    const KIMI_ALIASES: &[&str] = &["kimi-k2-5", "kimi", "moonshot"];

    pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
        ModelId::KimiK2_5,
        Provider::Moonshot,
        "kimi-k2.5",
        "Kimi K2.5",
        true,
    )
    .with_aliases(KIMI_ALIASES)
    .with_openrouter_equivalent(ModelId::OrKimiK2_5)];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Moonshot, ModelId::KimiK2_5, MODELS);
}

mod openai {
    use super::*;

    const OPENCODE_ALIASES: &[&str] = &["opencode-cli"];
    const GPT_5_1_ALIASES: &[&str] = &["gpt-5-1"];
    const GPT_5_2_ALIASES: &[&str] = &["gpt-5-2"];
    const GPT_5_4_ALIASES: &[&str] = &["gpt-5-4"];
    const GPT_5_4_MINI_ALIASES: &[&str] = &["gpt-5-4-mini"];
    const GPT_5_4_NANO_ALIASES: &[&str] = &["gpt-5-4-nano"];
    const GPT_5_5_ALIASES: &[&str] = &["gpt-5-5"];
    const GPT_5_5_PRO_ALIASES: &[&str] = &["gpt-5-5-pro"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::Gpt5_4,
            Provider::OpenAI,
            "gpt-5.4",
            "GPT-5.4",
            false,
        )
        .with_aliases(GPT_5_4_ALIASES)
        .with_same_provider_fallback(ModelId::Gpt5_4Mini)
        .with_openrouter_equivalent(ModelId::OrGpt5),
        ModelDescriptor::new(
            ModelId::Gpt5_4Mini,
            Provider::OpenAI,
            "gpt-5.4-mini",
            "GPT-5.4 Mini",
            false,
        )
        .with_aliases(GPT_5_4_MINI_ALIASES)
        .with_same_provider_fallback(ModelId::Gpt5_4Nano)
        .with_openrouter_equivalent(ModelId::OrGpt5),
        ModelDescriptor::new(
            ModelId::Gpt5_4Nano,
            Provider::OpenAI,
            "gpt-5.4-nano",
            "GPT-5.4 Nano",
            false,
        )
        .with_aliases(GPT_5_4_NANO_ALIASES),
        ModelDescriptor::new(ModelId::Gpt5, Provider::OpenAI, "gpt-5", "GPT-5", false)
            .with_same_provider_fallback(ModelId::Gpt5Mini)
            .with_openrouter_equivalent(ModelId::OrGpt5),
        ModelDescriptor::new(
            ModelId::Gpt5Mini,
            Provider::OpenAI,
            "gpt-5-mini",
            "GPT-5 Mini",
            false,
        )
        .with_same_provider_fallback(ModelId::Gpt5Nano)
        .with_openrouter_equivalent(ModelId::OrGpt5),
        ModelDescriptor::new(
            ModelId::Gpt5Nano,
            Provider::OpenAI,
            "gpt-5-nano",
            "GPT-5 Nano",
            false,
        ),
        ModelDescriptor::new(
            ModelId::Gpt5Pro,
            Provider::OpenAI,
            "gpt-5-pro",
            "GPT-5 Pro",
            false,
        )
        .with_same_provider_fallback(ModelId::Gpt5)
        .with_openrouter_equivalent(ModelId::OrGpt5),
        ModelDescriptor::new(
            ModelId::Gpt5_1,
            Provider::OpenAI,
            "gpt-5.1",
            "GPT-5.1",
            false,
        )
        .with_aliases(GPT_5_1_ALIASES),
        ModelDescriptor::new(
            ModelId::Gpt5_2,
            Provider::OpenAI,
            "gpt-5.2",
            "GPT-5.2",
            false,
        )
        .with_aliases(GPT_5_2_ALIASES),
        ModelDescriptor::new(
            ModelId::Gpt5_5,
            Provider::OpenAI,
            "gpt-5.5",
            "GPT-5.5",
            false,
        )
        .with_aliases(GPT_5_5_ALIASES)
        .with_same_provider_fallback(ModelId::Gpt5_4)
        .with_openrouter_equivalent(ModelId::OrGpt5),
        ModelDescriptor::new(
            ModelId::Gpt5_5Pro,
            Provider::OpenAI,
            "gpt-5.5-pro",
            "GPT-5.5 Pro",
            false,
        )
        .with_aliases(GPT_5_5_PRO_ALIASES)
        .with_same_provider_fallback(ModelId::Gpt5_5),
        ModelDescriptor::new(
            ModelId::OpenCodeCli,
            Provider::OpenAI,
            "opencode",
            "OpenCode CLI",
            false,
        )
        .with_aliases(OPENCODE_ALIASES)
        .with_client_kind(ClientKind::OpenCodeCli),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::OpenAI, ModelId::Gpt5_5, MODELS);
}

mod openrouter {
    use super::*;

    const OPENROUTER_AUTO_ALIASES: &[&str] = &["openrouter"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::OpenRouterAuto,
            Provider::OpenRouter,
            "openrouter/auto",
            "OpenRouter Auto",
            true,
        )
        .with_aliases(OPENROUTER_AUTO_ALIASES),
        ModelDescriptor::new(
            ModelId::OrClaudeOpus4_6,
            Provider::OpenRouter,
            "anthropic/claude-opus-4.6",
            "OR Claude Opus 4.6",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrGpt5,
            Provider::OpenRouter,
            "openai/gpt-5",
            "OR GPT-5",
            false,
        ),
        ModelDescriptor::new(
            ModelId::OrGemini3Pro,
            Provider::OpenRouter,
            "google/gemini-3-pro-preview",
            "OR Gemini 3 Pro",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrDeepseekV3_2,
            Provider::OpenRouter,
            "deepseek/deepseek-v3.2",
            "OR DeepSeek V3.2",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrGrok4,
            Provider::OpenRouter,
            "x-ai/grok-4",
            "OR Grok 4",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrLlama4Maverick,
            Provider::OpenRouter,
            "meta-llama/llama-4-maverick",
            "OR Llama 4 Maverick",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrQwen3Coder,
            Provider::OpenRouter,
            "qwen/qwen3-coder",
            "OR Qwen3 Coder",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrDevstral2,
            Provider::OpenRouter,
            "mistralai/devstral-2-2512",
            "OR Devstral 2",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrGlm4_7,
            Provider::OpenRouter,
            "z-ai/glm-4.7",
            "OR GLM-4.7",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrKimiK2_5,
            Provider::OpenRouter,
            "moonshotai/kimi-k2.5",
            "OR Kimi K2.5",
            true,
        ),
        ModelDescriptor::new(
            ModelId::OrMinimaxM2_1,
            Provider::OpenRouter,
            "minimax/minimax-m2.1",
            "OR MiniMax M2.1",
            true,
        ),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::OpenRouter, ModelId::OrClaudeOpus4_6, MODELS);
}

mod qwen {
    use super::*;

    const QWEN3_MAX_ALIASES: &[&str] = &["qwen-max", "qwen"];
    const QWEN3_PLUS_ALIASES: &[&str] = &["qwen-plus"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::Qwen3Max,
            Provider::Qwen,
            "qwen3-max",
            "Qwen3 Max",
            true,
        )
        .with_aliases(QWEN3_MAX_ALIASES)
        .with_openrouter_equivalent(ModelId::OrQwen3Coder),
        ModelDescriptor::new(
            ModelId::Qwen3Plus,
            Provider::Qwen,
            "qwen3-plus",
            "Qwen3 Plus",
            true,
        )
        .with_aliases(QWEN3_PLUS_ALIASES)
        .with_openrouter_equivalent(ModelId::OrQwen3Coder),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Qwen, ModelId::Qwen3Max, MODELS);
}

mod siliconflow {
    use super::*;

    const SILICONFLOW_AUTO_ALIASES: &[&str] = &["siliconflow"];

    pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
        ModelId::SiliconFlowAuto,
        Provider::SiliconFlow,
        "deepseek-ai/DeepSeek-V3",
        "SiliconFlow Auto",
        true,
    )
    .with_aliases(SILICONFLOW_AUTO_ALIASES)];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::SiliconFlow, ModelId::SiliconFlowAuto, MODELS);
}

mod xai {
    use super::*;

    const GROK4_ALIASES: &[&str] = &["grok4"];
    const GROK3_MINI_ALIASES: &[&str] = &["grok3-mini"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(ModelId::Grok4, Provider::XAI, "grok-4", "Grok 4", true)
            .with_aliases(GROK4_ALIASES)
            .with_same_provider_fallback(ModelId::Grok3Mini)
            .with_openrouter_equivalent(ModelId::OrGrok4),
        ModelDescriptor::new(
            ModelId::Grok3Mini,
            Provider::XAI,
            "grok-3-mini",
            "Grok 3 Mini",
            true,
        )
        .with_aliases(GROK3_MINI_ALIASES)
        .with_openrouter_equivalent(ModelId::OrGrok4),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::XAI, ModelId::Grok4, MODELS);
}

mod yi {
    use super::*;

    const YI_ALIASES: &[&str] = &["yi"];

    pub const MODELS: &[ModelDescriptor] = &[ModelDescriptor::new(
        ModelId::YiLightning,
        Provider::Yi,
        "yi-lightning",
        "Yi Lightning",
        true,
    )
    .with_aliases(YI_ALIASES)];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::Yi, ModelId::YiLightning, MODELS);
}

mod zai {
    use super::*;

    const ZAI_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
    const GLM5_ALIASES: &[&str] = &["glm5"];
    const GLM5_TURBO_ALIASES: &[&str] = &["glm5-turbo"];
    const GLM5_CODE_ALIASES: &[&str] = &["glm5-code"];
    const GLM47_ALIASES: &[&str] = &["glm-4.7", "glm-4-7", "glm"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(ModelId::Glm5, Provider::Zai, "glm-5", "GLM-5", true)
            .with_aliases(GLM5_ALIASES)
            .with_canonical_family("glm-5")
            .with_same_provider_fallback(ModelId::Glm5Turbo)
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ModelDescriptor::new(
            ModelId::Glm5Turbo,
            Provider::Zai,
            "glm-5-turbo",
            "GLM-5 Turbo",
            true,
        )
        .with_aliases(GLM5_TURBO_ALIASES)
        .with_canonical_family("glm-5-turbo")
        .with_same_provider_fallback(ModelId::Glm5Code)
        .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ModelDescriptor::new(
            ModelId::Glm5Code,
            Provider::Zai,
            "glm-5",
            "GLM-5 Code",
            true,
        )
        .with_aliases(GLM5_CODE_ALIASES)
        .with_base_url_override(ZAI_CODING_BASE_URL)
        .with_canonical_family("glm-5-code")
        .with_same_provider_fallback(ModelId::Glm4_7)
        .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ModelDescriptor::new(ModelId::Glm4_7, Provider::Zai, "glm-4.7", "GLM-4.7", true)
            .with_aliases(GLM47_ALIASES)
            .with_canonical_family("glm-4-7")
            .with_openrouter_equivalent(ModelId::OrGlm4_7),
    ];

    pub const CATALOG: ProviderCatalog = ProviderCatalog::new(Provider::Zai, ModelId::Glm5, MODELS);
}

mod zai_coding_plan {
    use super::*;

    const ZAI_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
    const GLM5_1_ALIASES: &[&str] = &[
        "glm5.1",
        "glm-5.1",
        "glm5.1 coding plan",
        "glm-5.1 coding plan",
        "zai-coding-plan-glm5.1",
        "zai-coding-plan-glm-5.1",
    ];
    const GLM5_ALIASES: &[&str] = &[
        "glm5",
        "glm5 coding plan",
        "glm-5 coding plan",
        "zai-coding-plan",
        "zai-coding-plan-glm5",
        "zai-coding-plan-glm-5",
    ];
    const GLM5_TURBO_ALIASES: &[&str] = &[
        "glm5-turbo",
        "glm5 turbo coding plan",
        "glm-5 turbo coding plan",
        "glm-5-turbo coding plan",
        "zai-coding-plan-glm5-turbo",
        "zai-coding-plan-glm-5-turbo",
    ];
    const GLM5_CODE_ALIASES: &[&str] = &[
        "glm5-code",
        "glm5 coding plan code",
        "glm-5 coding plan code",
        "glm-5 coding-plan code",
        "zai-coding-plan-glm-5-code",
    ];
    const GLM47_ALIASES: &[&str] = &["glm-4.7", "zai-coding-plan-glm-4-7"];

    pub const MODELS: &[ModelDescriptor] = &[
        ModelDescriptor::new(
            ModelId::Glm5_1CodingPlan,
            Provider::ZaiCodingPlan,
            "glm-5.1",
            "GLM-5.1 (Coding Plan)",
            true,
        )
        .with_aliases(GLM5_1_ALIASES)
        .with_base_url_override(ZAI_CODING_BASE_URL)
        .with_canonical_family("glm-5-1")
        .with_same_provider_fallback(ModelId::Glm5CodingPlan),
        ModelDescriptor::new(
            ModelId::Glm5CodingPlan,
            Provider::ZaiCodingPlan,
            "glm-5",
            "GLM-5 (Coding Plan)",
            true,
        )
        .with_aliases(GLM5_ALIASES)
        .with_base_url_override(ZAI_CODING_BASE_URL)
        .with_canonical_family("glm-5")
        .with_same_provider_fallback(ModelId::Glm5TurboCodingPlan)
        .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ModelDescriptor::new(
            ModelId::Glm5TurboCodingPlan,
            Provider::ZaiCodingPlan,
            "glm-5-turbo",
            "GLM-5 Turbo (Coding Plan)",
            true,
        )
        .with_aliases(GLM5_TURBO_ALIASES)
        .with_base_url_override(ZAI_CODING_BASE_URL)
        .with_canonical_family("glm-5-turbo")
        .with_same_provider_fallback(ModelId::Glm5CodeCodingPlan)
        .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ModelDescriptor::new(
            ModelId::Glm5CodeCodingPlan,
            Provider::ZaiCodingPlan,
            "glm-5",
            "GLM-5 Code (Coding Plan)",
            true,
        )
        .with_aliases(GLM5_CODE_ALIASES)
        .with_base_url_override(ZAI_CODING_BASE_URL)
        .with_canonical_family("glm-5-code")
        .with_same_provider_fallback(ModelId::Glm4_7CodingPlan)
        .with_openrouter_equivalent(ModelId::OrGlm4_7),
        ModelDescriptor::new(
            ModelId::Glm4_7CodingPlan,
            Provider::ZaiCodingPlan,
            "glm-4.7",
            "GLM-4.7 (Coding Plan)",
            true,
        )
        .with_aliases(GLM47_ALIASES)
        .with_base_url_override(ZAI_CODING_BASE_URL)
        .with_canonical_family("glm-4-7")
        .with_openrouter_equivalent(ModelId::OrGlm4_7),
    ];

    pub const CATALOG: ProviderCatalog =
        ProviderCatalog::new(Provider::ZaiCodingPlan, ModelId::Glm5_1CodingPlan, MODELS);
}
