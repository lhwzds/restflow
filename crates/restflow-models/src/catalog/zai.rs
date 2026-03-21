use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

const ZAI_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
const GLM5_ALIASES: &[&str] = &["glm5"];
const GLM5_TURBO_ALIASES: &[&str] = &["glm5-turbo"];
const GLM5_CODE_ALIASES: &[&str] = &["glm5-code"];
const GLM47_ALIASES: &[&str] = &["glm-4.7"];

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
