use super::{ModelDescriptor, ProviderCatalog};
use crate::models::{ModelId, Provider};

const ZAI_CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
const GLM5_ALIASES: &[&str] = &["glm5", "zai-coding-plan-glm-5"];
const GLM5_TURBO_ALIASES: &[&str] = &["glm5-turbo", "zai-coding-plan-glm-5-turbo"];
const GLM5_CODE_ALIASES: &[&str] = &["glm5-code", "zai-coding-plan-glm-5-code"];
const GLM47_ALIASES: &[&str] = &["glm-4.7", "zai-coding-plan-glm-4-7"];

pub const MODELS: &[ModelDescriptor] = &[
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
    ProviderCatalog::new(Provider::ZaiCodingPlan, ModelId::Glm5CodingPlan, MODELS);
