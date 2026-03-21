use super::{ModelDescriptor, ProviderCatalog};
use crate::{ModelId, Provider};

pub const MODELS: &[ModelDescriptor] = &[
    ModelDescriptor::new(
        ModelId::OpenRouterAuto,
        Provider::OpenRouter,
        "openrouter/auto",
        "OpenRouter Auto",
        true,
    ),
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
