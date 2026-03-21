use std::sync::Arc;

use crate::error::{AiError, Result};
use crate::llm::{LlmClient, LlmClientFactory, LlmProvider};
use restflow_models::provider_meta;
use restflow_traits::ModelProvider;

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
Try one of: openai-codex, anthropic, deepseek, google, groq, openrouter, xai, qwen, zai, minimax."
        ))
    })?;
    let actual_provider = factory
        .provider_for_model(&resolved_model)
        .ok_or_else(|| AiError::Agent(format!("Unknown model for sub-agent: {resolved_model}")))?;
    if actual_provider != requested_provider {
        return Err(AiError::Agent(format!(
            "Model '{resolved_model}' does not belong to provider '{provider_selector}' (actual: '{}').",
            actual_provider.as_str()
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

    if let Some(exact) = available
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(query))
    {
        return Ok(exact.clone());
    }

    if factory.provider_for_model(query).is_some() {
        return Ok(query.to_string());
    }

    let normalized_query = normalize_model_identifier(query);
    if normalized_query.is_empty() {
        return Err(AiError::Agent(format!(
            "Unknown model for sub-agent: {model}"
        )));
    }

    let normalized_exact_matches: Vec<&String> = available
        .iter()
        .filter(|candidate| normalize_model_identifier(candidate) == normalized_query)
        .collect();
    if normalized_exact_matches.len() == 1 {
        return Ok(normalized_exact_matches[0].clone());
    }

    if let Some((provider, model_name)) = query.split_once(':') {
        let provider_joined = normalize_model_identifier(&format!("{provider}-{model_name}"));
        let canonical_matches: Vec<&String> = available
            .iter()
            .filter(|candidate| normalize_model_identifier(candidate) == provider_joined)
            .collect();
        if canonical_matches.len() == 1 {
            return Ok(canonical_matches[0].clone());
        }
    }

    if let Some(alias_resolved) = resolve_model_alias(&normalized_query, &available) {
        return Ok(alias_resolved);
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

fn resolve_model_alias(normalized_query: &str, available: &[String]) -> Option<String> {
    let mut normalized_available = available
        .iter()
        .map(|candidate| (normalize_model_identifier(candidate), candidate.clone()))
        .collect::<Vec<(String, String)>>();

    if matches!(
        normalized_query,
        "minimax-coding-plan" | "minimax-coding" | "coding-plan-minimax"
    ) || normalized_query.starts_with("minimax-coding-plan")
    {
        let mut matches = normalized_available
            .iter()
            .filter(|(normalized, _)| normalized.starts_with("minimax-coding-plan-"))
            .map(|(_, original)| original.clone())
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return None;
        }
        matches.sort();
        return matches
            .iter()
            .find(|candidate| candidate.contains("m2-5"))
            .cloned()
            .or_else(|| matches.last().cloned());
    }

    if matches!(
        normalized_query,
        "glm5-coding-plan"
            | "glm-5-coding-plan"
            | "zai-coding-plan"
            | "zai-coding-plan-glm5"
            | "zai-coding-plan-glm-5"
    ) && let Some(exact) = normalized_available
        .iter()
        .find(|(normalized, _)| normalized == "zai-coding-plan-glm-5")
        .map(|(_, original)| original.clone())
    {
        return Some(exact);
    }

    if matches!(
        normalized_query,
        "glm5-turbo-coding-plan"
            | "glm-5-turbo-coding-plan"
            | "zai-coding-plan-glm5-turbo"
            | "zai-coding-plan-glm-5-turbo"
    ) && let Some(exact) = normalized_available
        .iter()
        .find(|(normalized, _)| normalized == "zai-coding-plan-glm-5-turbo")
        .map(|(_, original)| original.clone())
    {
        return Some(exact);
    }

    if matches!(
        normalized_query,
        "glm5-coding-plan-code" | "glm-5-coding-plan-code"
    ) && let Some(exact) = normalized_available
        .iter()
        .find(|(normalized, _)| normalized == "zai-coding-plan-glm-5-code")
        .map(|(_, original)| original.clone())
    {
        return Some(exact);
    }

    let mut prefix_matches = normalized_available
        .drain(..)
        .filter(|(normalized, _)| normalized.starts_with(normalized_query))
        .map(|(_, original)| original)
        .collect::<Vec<_>>();
    if prefix_matches.is_empty() {
        return None;
    }
    prefix_matches.sort();
    Some(prefix_matches[0].clone())
}

fn parse_provider_selector(value: &str) -> Option<LlmProvider> {
    let normalized = normalize_model_identifier(value);
    if matches!(
        normalized.as_str(),
        "openai-codex" | "codex" | "codex-cli" | "claude-code" | "gemini-cli"
    ) {
        return Some(match normalized.as_str() {
            "claude-code" => LlmProvider::Anthropic,
            "gemini-cli" => LlmProvider::Google,
            _ => LlmProvider::OpenAI,
        });
    }

    let provider = ModelProvider::parse_alias(&normalized)?;
    Some(provider_meta(provider).runtime_provider)
}

fn normalize_model_identifier(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_dash = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }
        if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }

    normalized.trim_matches('-').to_string()
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
        assert_eq!(parse_provider_selector("gpt"), Some(LlmProvider::OpenAI));
        assert_eq!(parse_provider_selector("gemini"), Some(LlmProvider::Google));
        assert_eq!(
            parse_provider_selector("zhipu-coding-plan"),
            Some(LlmProvider::ZaiCodingPlan)
        );
        assert_eq!(
            parse_provider_selector("minimax-coding"),
            Some(LlmProvider::MiniMaxCodingPlan)
        );
        assert_eq!(
            parse_provider_selector("openai-codex"),
            Some(LlmProvider::OpenAI)
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
