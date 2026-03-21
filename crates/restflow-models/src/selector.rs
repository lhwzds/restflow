use restflow_traits::ModelProvider;

use crate::{ClientKind, LlmProvider, ModelId, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderSelector {
    Provider(Provider),
    ClientKind(ClientKind),
}

impl ProviderSelector {
    pub fn label(self) -> &'static str {
        match self {
            Self::Provider(Provider::ClaudeCode) | Self::ClientKind(ClientKind::ClaudeCodeCli) => {
                "claude-code"
            }
            Self::Provider(Provider::Codex) | Self::ClientKind(ClientKind::CodexCli) => {
                "openai-codex"
            }
            Self::Provider(provider) => provider.as_canonical_str(),
            Self::ClientKind(ClientKind::Http) => "http",
            Self::ClientKind(ClientKind::OpenCodeCli) => "opencode-cli",
            Self::ClientKind(ClientKind::GeminiCli) => "gemini-cli",
        }
    }

    pub fn matches_model(self, model: ModelId) -> bool {
        match self {
            Self::Provider(provider) => model.provider() == provider,
            Self::ClientKind(client_kind) => model.client_kind() == client_kind,
        }
    }

    pub fn runtime_provider(self) -> Option<LlmProvider> {
        match self {
            Self::Provider(provider) => Some(provider.as_llm_provider()),
            Self::ClientKind(ClientKind::CodexCli | ClientKind::OpenCodeCli) => {
                Some(LlmProvider::OpenAI)
            }
            Self::ClientKind(ClientKind::GeminiCli) => Some(LlmProvider::Google),
            Self::ClientKind(ClientKind::ClaudeCodeCli) => Some(LlmProvider::Anthropic),
            Self::ClientKind(ClientKind::Http) => None,
        }
    }
}

pub fn parse_provider_selector(value: &str) -> Option<ProviderSelector> {
    let normalized = normalize_identifier(value);
    let special = match normalized.as_str() {
        "claude-code" | "claudecode" => Some(ProviderSelector::Provider(Provider::ClaudeCode)),
        "codex" | "codex-cli" | "codexcli" | "openai-codex" | "openaicodex" => {
            Some(ProviderSelector::Provider(Provider::Codex))
        }
        "opencode" | "opencode-cli" | "opencodecli" => {
            Some(ProviderSelector::ClientKind(ClientKind::OpenCodeCli))
        }
        "gemini-cli" | "geminicli" => Some(ProviderSelector::ClientKind(ClientKind::GeminiCli)),
        _ => None,
    };
    special.or_else(|| {
        ModelProvider::parse_alias(value)
            .map(Provider::from_model_provider)
            .map(ProviderSelector::Provider)
    })
}

pub fn split_provider_qualified_model(value: &str) -> Option<(ProviderSelector, &str)> {
    for separator in [':', '/'] {
        let Some((provider_raw, model_raw)) = value.split_once(separator) else {
            continue;
        };
        let model_raw = model_raw.trim();
        if model_raw.is_empty() {
            continue;
        }
        if let Some(provider) = parse_provider_selector(provider_raw) {
            return Some((provider, model_raw));
        }
    }

    None
}

pub fn parse_model_reference(value: &str) -> Option<ModelId> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    parse_plain_model_reference(trimmed).or_else(|| {
        let (selector, model_raw) = split_provider_qualified_model(trimmed)?;
        match selector {
            ProviderSelector::Provider(provider) => {
                ModelId::for_provider_and_model(provider, model_raw)
            }
            ProviderSelector::ClientKind(client_kind) => parse_plain_model_reference(model_raw)
                .filter(|model| model.client_kind() == client_kind),
        }
    })
}

pub fn resolve_available_model_name(requested: &str, available: &[String]) -> Option<String> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(exact) = find_case_insensitive_match(available, trimmed) {
        return Some(exact);
    }

    if let Some(model) = parse_model_reference(trimmed)
        && let Some(resolved) = find_available_model_for_id(available, model)
    {
        return Some(resolved);
    }

    let normalized = normalize_identifier(trimmed);
    if normalized.is_empty() {
        return None;
    }

    let normalized_matches = available
        .iter()
        .filter(|candidate| normalize_identifier(candidate) == normalized)
        .collect::<Vec<_>>();
    if normalized_matches.len() == 1 {
        return Some(normalized_matches[0].clone());
    }

    let prefix_matches = available
        .iter()
        .filter(|candidate| normalize_identifier(candidate).starts_with(&normalized))
        .collect::<Vec<_>>();
    if prefix_matches.is_empty() {
        return None;
    }

    let mut sorted_matches = prefix_matches.into_iter().cloned().collect::<Vec<_>>();
    sorted_matches.sort();
    sorted_matches.into_iter().next()
}

fn parse_plain_model_reference(value: &str) -> Option<ModelId> {
    ModelId::from_api_name(value)
        .or_else(|| ModelId::from_canonical_id(value))
        .or_else(|| ModelId::from_serialized_str(value))
}

fn find_available_model_for_id(available: &[String], model: ModelId) -> Option<String> {
    find_case_insensitive_match(available, model.as_serialized_str())
        .or_else(|| find_case_insensitive_match(available, model.as_str()))
        .or_else(|| {
            available.iter().find_map(|candidate| {
                (parse_plain_model_reference(candidate) == Some(model)).then(|| candidate.clone())
            })
        })
}

fn find_case_insensitive_match(available: &[String], requested: &str) -> Option<String> {
    available
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(requested))
        .cloned()
}

fn normalize_identifier(value: &str) -> String {
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
    use super::{
        ProviderSelector, parse_model_reference, parse_provider_selector,
        resolve_available_model_name, split_provider_qualified_model,
    };
    use crate::{ClientKind, ModelId, Provider};

    #[test]
    fn parse_provider_selector_supports_shared_and_special_aliases() {
        assert_eq!(
            parse_provider_selector("openai-codex"),
            Some(ProviderSelector::Provider(Provider::Codex))
        );
        assert_eq!(
            parse_provider_selector("claude-code"),
            Some(ProviderSelector::Provider(Provider::ClaudeCode))
        );
        assert_eq!(
            parse_provider_selector("opencode-cli"),
            Some(ProviderSelector::ClientKind(ClientKind::OpenCodeCli))
        );
        assert_eq!(
            parse_provider_selector("gemini-cli"),
            Some(ProviderSelector::ClientKind(ClientKind::GeminiCli))
        );
        assert_eq!(
            parse_provider_selector("gpt"),
            Some(ProviderSelector::Provider(Provider::OpenAI))
        );
    }

    #[test]
    fn split_provider_qualified_model_accepts_special_selectors() {
        assert_eq!(
            split_provider_qualified_model("openai-codex:gpt-5.3-codex"),
            Some((ProviderSelector::Provider(Provider::Codex), "gpt-5.3-codex"))
        );
        assert_eq!(
            split_provider_qualified_model("gemini-cli/gemini-cli"),
            Some((
                ProviderSelector::ClientKind(ClientKind::GeminiCli),
                "gemini-cli"
            ))
        );
    }

    #[test]
    fn parse_model_reference_supports_provider_qualified_aliases() {
        assert_eq!(
            parse_model_reference("openai-codex:gpt-5.3-codex"),
            Some(ModelId::CodexCli)
        );
        assert_eq!(
            parse_model_reference("claude-code:sonnet"),
            Some(ModelId::ClaudeCodeSonnet)
        );
    }

    #[test]
    fn resolve_available_model_name_uses_shared_catalog_aliases() {
        assert_eq!(
            resolve_available_model_name(
                "minimax/coding-plan",
                &[
                    "minimax-coding-plan-m2-1".to_string(),
                    "minimax-coding-plan-m2-5".to_string(),
                ]
            ),
            Some("minimax-coding-plan-m2-5".to_string())
        );
        assert_eq!(
            resolve_available_model_name(
                "glm5 turbo coding plan",
                &[
                    "zai-coding-plan-glm-5".to_string(),
                    "zai-coding-plan-glm-5-turbo".to_string(),
                ]
            ),
            Some("zai-coding-plan-glm-5-turbo".to_string())
        );
    }
}
