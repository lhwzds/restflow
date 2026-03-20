mod anthropic;
mod claude_code;
mod codex;
mod deepseek;
mod doubao;
mod google;
mod groq;
mod minimax;
mod minimax_coding_plan;
mod moonshot;
mod openai;
mod openrouter;
mod qwen;
mod siliconflow;
mod xai;
mod yi;
mod zai;
mod zai_coding_plan;

use std::collections::HashMap;
use std::sync::OnceLock;

use super::ai_model::{ModelId, ModelMetadata, ModelMetadataDTO, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub provider: Provider,
    pub api_name: &'static str,
    pub display_name: &'static str,
    pub supports_temperature: bool,
    pub aliases: &'static [&'static str],
    pub canonical_family: Option<&'static str>,
    pub same_provider_fallback: Option<ModelId>,
    pub openrouter_equivalent: Option<ModelId>,
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
            canonical_family: None,
            same_provider_fallback: None,
            openrouter_equivalent: None,
        }
    }

    pub const fn with_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
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
    claude_code::CATALOG,
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
    NAME_LOOKUP
        .get_or_init(|| {
            let mut lookup = HashMap::new();
            for descriptor in all_descriptors() {
                register_lookup_key(
                    &mut lookup,
                    descriptor.id.as_serialized_str(),
                    descriptor.id,
                );
                register_lookup_key(&mut lookup, descriptor.api_name, descriptor.id);
                for alias in descriptor.aliases {
                    register_lookup_key(&mut lookup, alias, descriptor.id);
                }
            }
            lookup
        })
        .get(&key)
        .copied()
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
