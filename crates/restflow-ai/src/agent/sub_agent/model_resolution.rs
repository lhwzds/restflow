use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::{LlmClient, LlmClientFactory};
use restflow_models::{
    parse_model_reference, parse_provider_selector, resolve_available_model_name,
};

pub(crate) fn resolve_llm_client(
    request_model: Option<&str>,
    request_provider: Option<&str>,
    def_default_model: Option<&str>,
    parent_client: &Arc<dyn LlmClient>,
    factory: Option<&Arc<dyn LlmClientFactory>>,
) -> Result<Arc<dyn LlmClient>> {
    let chosen_model = request_model.or(def_default_model);
    let Some(model) = chosen_model else {
        return Ok(parent_client.clone());
    };
    let Some(factory) = factory else {
        return Ok(parent_client.clone());
    };

    let resolved_model = resolve_model_with_provider(model, request_provider, factory.as_ref())?;
    let provider = factory
        .provider_for_model(&resolved_model)
        .ok_or_else(|| AiError::Agent(format!("Unknown model for sub-agent: {model}")))?;
    let api_key = factory.resolve_api_key(provider);
    factory.create_client(&resolved_model, api_key.as_deref())
}

pub(crate) fn resolve_model_with_provider(
    model: &str,
    provider: Option<&str>,
    factory: &dyn LlmClientFactory,
) -> Result<String> {
    let resolved_model = resolve_model_name(model, factory)?;
    let Some(provider_selector) = provider.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(resolved_model);
    };

    let requested_provider = parse_provider_selector(provider_selector).ok_or_else(|| {
        AiError::Agent(format!(
            "Unknown provider for sub-agent: {provider_selector}. \
Try one of: openai-codex, anthropic, deepseek, google, groq, openrouter, xai, qwen, zai, minimax, opencode-cli, gemini-cli."
        ))
    })?;
    let provider_matches = parse_model_reference(&resolved_model)
        .map(|model_id| requested_provider.matches_model(model_id))
        .unwrap_or_else(|| {
            factory
                .provider_for_model(&resolved_model)
                .zip(requested_provider.runtime_provider())
                .map(|(actual, expected)| actual == expected)
                .unwrap_or(false)
        });
    if !provider_matches {
        let actual_provider = parse_model_reference(&resolved_model)
            .map(|model_id| model_id.provider().as_canonical_str().to_string())
            .or_else(|| {
                factory
                    .provider_for_model(&resolved_model)
                    .map(|provider| provider.as_str().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());
        return Err(AiError::Agent(format!(
            "Model '{resolved_model}' does not belong to provider '{provider_selector}' (actual: '{}').",
            actual_provider
        )));
    }

    Ok(resolved_model)
}

fn resolve_model_name(model: &str, factory: &dyn LlmClientFactory) -> Result<String> {
    let query = model.trim();
    if query.is_empty() {
        return Err(AiError::Agent(
            "Unknown model for sub-agent: empty model".to_string(),
        ));
    }

    let available = factory.available_models();
    if available.is_empty() {
        return Err(AiError::Agent(format!(
            "Unknown model for sub-agent: {model}. No model catalog is available."
        )));
    }

    if let Some(resolved) = resolve_available_model_name(query, &available) {
        return Ok(resolved);
    }

    let suggestions = available
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    Err(AiError::Agent(format!(
        "Unknown model for sub-agent: {model}. Try one of: {suggestions}"
    )))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::llm::{ClientKind, LlmClient, LlmProvider};

    use super::*;

    struct AliasOnlyFactory {
        models: Vec<String>,
    }

    impl AliasOnlyFactory {
        fn new(models: Vec<&str>) -> Self {
            Self {
                models: models.into_iter().map(str::to_string).collect(),
            }
        }
    }

    impl LlmClientFactory for AliasOnlyFactory {
        fn create_client(
            &self,
            _model: &str,
            _api_key: Option<&str>,
        ) -> Result<Arc<dyn LlmClient>> {
            Err(AiError::Llm(
                "create_client is not used in alias tests".to_string(),
            ))
        }

        fn available_models(&self) -> Vec<String> {
            self.models.clone()
        }

        fn resolve_api_key(&self, _provider: LlmProvider) -> Option<String> {
            None
        }

        fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
            self.models
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(model.trim()))
                .map(|_| LlmProvider::OpenAI)
        }

        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
            self.models
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(model.trim()))
                .then_some(ClientKind::Http)
        }
    }

    #[test]
    fn parse_provider_selector_accepts_shared_aliases() {
        assert_eq!(
            parse_provider_selector("gpt").map(|selector| selector.label()),
            Some("openai")
        );
        assert_eq!(
            parse_provider_selector("gemini").map(|selector| selector.label()),
            Some("google")
        );
        assert_eq!(
            parse_provider_selector("zhipu-coding-plan").map(|selector| selector.label()),
            Some("zai-coding-plan")
        );
        assert_eq!(
            parse_provider_selector("minimax-coding").map(|selector| selector.label()),
            Some("minimax-coding-plan")
        );
        assert_eq!(
            parse_provider_selector("openai-codex").map(|selector| selector.label()),
            Some("openai-codex")
        );
    }

    #[test]
    fn resolve_model_name_accepts_case_insensitive_match() {
        let factory = AliasOnlyFactory::new(vec!["gpt-5", "minimax-coding-plan-m2-5"]);
        let resolved = resolve_model_name("GPT-5", &factory).unwrap();
        assert_eq!(resolved, "gpt-5");
    }

    #[test]
    fn resolve_model_name_maps_minimax_coding_plan_alias() {
        let factory =
            AliasOnlyFactory::new(vec!["minimax-coding-plan-m2-1", "minimax-coding-plan-m2-5"]);
        let resolved = resolve_model_name("minimax/coding-plan", &factory).unwrap();
        assert_eq!(resolved, "minimax-coding-plan-m2-5");
    }

    #[test]
    fn resolve_model_name_maps_glm5_coding_plan_alias() {
        let factory =
            AliasOnlyFactory::new(vec!["zai-coding-plan-glm-5", "zai-coding-plan-glm-5-code"]);
        let resolved = resolve_model_name("glm5 coding plan", &factory).unwrap();
        assert_eq!(resolved, "zai-coding-plan-glm-5");
    }

    #[test]
    fn resolve_model_name_maps_glm5_coding_plan_code_alias() {
        let factory =
            AliasOnlyFactory::new(vec!["zai-coding-plan-glm-5", "zai-coding-plan-glm-5-code"]);
        let resolved = resolve_model_name("glm-5 coding-plan code", &factory).unwrap();
        assert_eq!(resolved, "zai-coding-plan-glm-5-code");
    }

    #[test]
    fn resolve_model_name_maps_glm5_turbo_coding_plan_alias() {
        let factory = AliasOnlyFactory::new(vec![
            "zai-coding-plan-glm-5",
            "zai-coding-plan-glm-5-turbo",
            "zai-coding-plan-glm-5-code",
        ]);
        let resolved = resolve_model_name("glm5 turbo coding plan", &factory).unwrap();
        assert_eq!(resolved, "zai-coding-plan-glm-5-turbo");
    }

    #[test]
    fn resolve_model_name_returns_helpful_error_for_unknown_model() {
        let factory = AliasOnlyFactory::new(vec!["gpt-5", "minimax-coding-plan-m2-5"]);
        let error = resolve_model_name("unknown-model", &factory).unwrap_err();
        assert!(error.to_string().contains("Try one of"));
    }

    #[test]
    fn resolve_model_with_provider_accepts_codex_provider_alias() {
        let factory = AliasOnlyFactory::new(vec!["gpt-5.3-codex"]);
        let resolved =
            resolve_model_with_provider("gpt-5.3-codex", Some("openai-codex"), &factory).unwrap();
        assert_eq!(resolved, "gpt-5.3-codex");
    }

    #[test]
    fn resolve_model_with_provider_rejects_mismatch() {
        let factory = AliasOnlyFactory::new(vec!["gpt-5.3-codex"]);
        let error =
            resolve_model_with_provider("gpt-5.3-codex", Some("anthropic"), &factory).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not belong to provider 'anthropic'")
        );
    }
}
