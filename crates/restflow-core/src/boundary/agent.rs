use crate::models::{
    AgentNode, ApiKeyConfig, CodexCliExecutionMode, ModelRef, ModelRoutingConfig,
    SkillPreflightPolicyMode, ValidationError,
};
use restflow_traits::request::{
    AgentNode as ContractAgentNode, ApiKeyConfig as ContractApiKeyConfig,
    CodexCliExecutionMode as ContractCodexCliExecutionMode,
    SkillPreflightPolicyMode as ContractSkillPreflightPolicyMode,
};

pub(crate) fn agent_to_contract(value: AgentNode) -> ContractAgentNode {
    ContractAgentNode {
        model_ref: value.model_ref.map(Into::into),
        prompt: value.prompt,
        temperature: value.temperature,
        codex_cli_reasoning_effort: value.codex_cli_reasoning_effort,
        codex_cli_execution_mode: value.codex_cli_execution_mode.map(Into::into),
        api_key_config: value.api_key_config.map(Into::into),
        tools: value.tools,
        skills: value.skills,
        skill_variables: value.skill_variables,
        skill_preflight_policy_mode: value.skill_preflight_policy_mode.map(Into::into),
        model_routing: value.model_routing.map(Into::into),
    }
}

pub(crate) fn agent_from_contract(
    value: ContractAgentNode,
) -> Result<AgentNode, Vec<ValidationError>> {
    let mut errors = Vec::new();

    let model_ref = match value.model_ref {
        Some(model_ref) => match ModelRef::try_from(model_ref) {
            Ok(model_ref) => Some(model_ref),
            Err(error) => {
                errors.push(error);
                None
            }
        },
        None => None,
    };

    let mut agent = AgentNode {
        model_ref,
        prompt: value.prompt,
        temperature: value.temperature,
        codex_cli_reasoning_effort: value.codex_cli_reasoning_effort,
        codex_cli_execution_mode: match value.codex_cli_execution_mode {
            Some(ContractCodexCliExecutionMode::Safe) => Some(CodexCliExecutionMode::Safe),
            Some(ContractCodexCliExecutionMode::Bypass) => Some(CodexCliExecutionMode::Bypass),
            Some(ContractCodexCliExecutionMode::Unknown) | None => None,
        },
        api_key_config: value.api_key_config.map(|config| match config {
            ContractApiKeyConfig::Direct(secret) => ApiKeyConfig::Direct(secret),
            ContractApiKeyConfig::Secret(secret) => ApiKeyConfig::Secret(secret),
        }),
        tools: value.tools,
        skills: value.skills,
        skill_variables: value.skill_variables,
        skill_preflight_policy_mode: value.skill_preflight_policy_mode.map(|mode| match mode {
            ContractSkillPreflightPolicyMode::Off => SkillPreflightPolicyMode::Off,
            ContractSkillPreflightPolicyMode::Warn => SkillPreflightPolicyMode::Warn,
            ContractSkillPreflightPolicyMode::Enforce => SkillPreflightPolicyMode::Enforce,
        }),
        model_routing: value.model_routing.map(|routing| ModelRoutingConfig {
            enabled: routing.enabled,
            routine_model: routing.routine_model,
            moderate_model: routing.moderate_model,
            complex_model: routing.complex_model,
            escalate_on_failure: routing.escalate_on_failure,
        }),
    };

    if errors.is_empty()
        && let Err(error) = agent.normalize_model_fields()
    {
        errors.push(error);
    }

    if errors.is_empty() {
        Ok(agent)
    } else {
        Err(errors)
    }
}

impl From<AgentNode> for ContractAgentNode {
    fn from(value: AgentNode) -> Self {
        agent_to_contract(value)
    }
}

impl TryFrom<ContractAgentNode> for AgentNode {
    type Error = Vec<ValidationError>;

    fn try_from(value: ContractAgentNode) -> Result<Self, Self::Error> {
        agent_from_contract(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelId;
    use restflow_traits::request::WireModelRef;

    #[test]
    fn agent_boundary_round_trips_contract_shape() {
        let agent = AgentNode {
            model_ref: Some(ModelRef::from_model(ModelId::Gpt5)),
            prompt: Some("prompt".to_string()),
            temperature: Some(0.2),
            codex_cli_reasoning_effort: Some("high".to_string()),
            codex_cli_execution_mode: Some(CodexCliExecutionMode::Safe),
            api_key_config: Some(ApiKeyConfig::Secret("OPENAI_API_KEY".to_string())),
            tools: Some(vec!["bash".to_string()]),
            skills: Some(vec!["skill-1".to_string()]),
            skill_variables: None,
            skill_preflight_policy_mode: Some(SkillPreflightPolicyMode::Warn),
            model_routing: Some(ModelRoutingConfig::default()),
        };

        let contract: ContractAgentNode = agent.clone().into();
        let decoded = AgentNode::try_from(contract).expect("agent boundary should decode");
        assert_eq!(decoded.model_ref, agent.model_ref);
    }

    #[test]
    fn agent_boundary_rejects_invalid_model_ref_pair() {
        let errors = AgentNode::try_from(ContractAgentNode {
            model_ref: Some(WireModelRef {
                provider: "openai".to_string(),
                model: "claude-sonnet-4".to_string(),
            }),
            ..ContractAgentNode::default()
        })
        .expect_err("invalid provider/model pair should fail");

        assert_eq!(errors[0].field, "model_ref.model");
    }
}
