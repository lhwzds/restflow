use super::ModelId;
use crate::catalog;

#[allow(non_upper_case_globals)]
impl ModelId {
    pub const Gpt5: Self = Self("gpt-5");
    pub const Gpt5Mini: Self = Self("gpt-5-mini");
    pub const Gpt5Nano: Self = Self("gpt-5-nano");
    pub const Gpt5Pro: Self = Self("gpt-5-pro");
    pub const Gpt5_1: Self = Self("gpt-5-1");
    pub const Gpt5_2: Self = Self("gpt-5-2");
    pub const ClaudeOpus4_6: Self = Self("claude-opus-4-6");
    pub const ClaudeSonnet4_5: Self = Self("claude-sonnet-4-5");
    pub const ClaudeHaiku4_5: Self = Self("claude-haiku-4-5");
    pub const ClaudeCodeOpus: Self = Self("claude-code-opus");
    pub const ClaudeCodeSonnet: Self = Self("claude-code-sonnet");
    pub const ClaudeCodeHaiku: Self = Self("claude-code-haiku");
    pub const DeepseekChat: Self = Self("deepseek-chat");
    pub const DeepseekReasoner: Self = Self("deepseek-reasoner");
    pub const Gemini25Pro: Self = Self("gemini-2-5-pro");
    pub const Gemini25Flash: Self = Self("gemini-2-5-flash");
    pub const Gemini3Pro: Self = Self("gemini-3-pro");
    pub const Gemini3Flash: Self = Self("gemini-3-flash");
    pub const GroqLlama4Scout: Self = Self("groq-llama4-scout");
    pub const GroqLlama4Maverick: Self = Self("groq-llama4-maverick");
    pub const Grok4: Self = Self("grok-4");
    pub const Grok3Mini: Self = Self("grok-3-mini");
    pub const OpenRouterAuto: Self = Self("openrouter");
    pub const OrClaudeOpus4_6: Self = Self("or-claude-opus-4-6");
    pub const OrGpt5: Self = Self("or-gpt-5");
    pub const OrGemini3Pro: Self = Self("or-gemini-3-pro");
    pub const OrDeepseekV3_2: Self = Self("or-deepseek-v3-2");
    pub const OrGrok4: Self = Self("or-grok-4");
    pub const OrLlama4Maverick: Self = Self("or-llama-4-maverick");
    pub const OrQwen3Coder: Self = Self("or-qwen3-coder");
    pub const OrDevstral2: Self = Self("or-devstral-2");
    pub const OrGlm4_7: Self = Self("or-glm-4-7");
    pub const OrKimiK2_5: Self = Self("or-kimi-k2-5");
    pub const OrMinimaxM2_1: Self = Self("or-minimax-m2-1");
    pub const Qwen3Max: Self = Self("qwen3-max");
    pub const Qwen3Plus: Self = Self("qwen3-plus");
    pub const Glm5: Self = Self("glm-5");
    pub const Glm5Turbo: Self = Self("glm-5-turbo");
    pub const Glm5Code: Self = Self("glm-5-code");
    pub const Glm4_7: Self = Self("glm-4-7");
    pub const Glm5_1CodingPlan: Self = Self("zai-coding-plan-glm-5-1");
    pub const Glm5CodingPlan: Self = Self("zai-coding-plan-glm-5");
    pub const Glm5TurboCodingPlan: Self = Self("zai-coding-plan-glm-5-turbo");
    pub const Glm5CodeCodingPlan: Self = Self("zai-coding-plan-glm-5-code");
    pub const Glm4_7CodingPlan: Self = Self("zai-coding-plan-glm-4-7");
    pub const KimiK2_5: Self = Self("kimi-k2-5");
    pub const DoubaoPro: Self = Self("doubao-pro");
    pub const YiLightning: Self = Self("yi-lightning");
    pub const SiliconFlowAuto: Self = Self("siliconflow");
    pub const MiniMaxM21: Self = Self("minimax-m2-1");
    pub const MiniMaxM25: Self = Self("minimax-m2-5");
    pub const MiniMaxM27: Self = Self("minimax-m2-7");
    pub const MiniMaxM27Highspeed: Self = Self("minimax-m2-7-highspeed");
    pub const MiniMaxM21CodingPlan: Self = Self("minimax-coding-plan-m2-1");
    pub const MiniMaxM25CodingPlan: Self = Self("minimax-coding-plan-m2-5");
    pub const MiniMaxM25CodingPlanHighspeed: Self = Self("minimax-coding-plan-m2-5-highspeed");
    pub const Gpt5_4Codex: Self = Self("gpt-5.4");
    pub const Gpt5_4MiniCodex: Self = Self("gpt-5.4-mini");
    pub const Gpt5Codex: Self = Self("gpt-5-codex");
    pub const Gpt5_1Codex: Self = Self("gpt-5.1-codex");
    pub const Gpt5_2Codex: Self = Self("gpt-5.2-codex");
    pub const CodexCli: Self = Self("gpt-5.3-codex");
    pub const OpenCodeCli: Self = Self("opencode-cli");
    pub const GeminiCli: Self = Self("gemini-cli");

    pub const fn as_serialized_str(&self) -> &'static str {
        self.0
    }

    pub fn from_serialized_str(value: &str) -> Option<Self> {
        let normalized = value.trim();
        if normalized.is_empty() {
            return None;
        }

        catalog::lookup_by_name(normalized)
    }

    pub fn all() -> &'static [Self] {
        catalog::all_model_ids()
    }
}
