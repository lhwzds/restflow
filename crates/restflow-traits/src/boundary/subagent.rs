use restflow_contracts::request::{
    InlineSubagentConfig as ContractInlineSubagentConfig, SpawnPriority as ContractSpawnPriority,
    SubagentSpawnRequest as ContractSubagentSpawnRequest,
};

use crate::{InlineSubagentConfig, SpawnPriority, SpawnRequest, SubagentDefSummary, ToolError};

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

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn normalize_model_provider_pair(
    model: Option<String>,
    model_provider: Option<String>,
) -> Result<(Option<String>, Option<String>), ToolError> {
    let model = normalize_optional_text(model);
    let model_provider = normalize_optional_text(model_provider);
    if model.is_some() != model_provider.is_some() {
        return Err(ToolError::Tool(
            "Model override requires both 'model' and 'provider' fields.".to_string(),
        ));
    }
    Ok((model, model_provider))
}

fn normalize_inline_config(
    inline: Option<ContractInlineSubagentConfig>,
) -> Option<InlineSubagentConfig> {
    let inline = inline?;
    let config = InlineSubagentConfig {
        name: inline.name,
        system_prompt: inline.system_prompt,
        allowed_tools: inline.allowed_tools,
        max_iterations: inline.max_iterations,
    };

    if config.name.is_none()
        && config.system_prompt.is_none()
        && config.allowed_tools.is_none()
        && config.max_iterations.is_none()
    {
        None
    } else {
        Some(config)
    }
}

pub fn resolve_agent_id(
    available_agents: &[SubagentDefSummary],
    requested: &str,
) -> Result<String, ToolError> {
    let query = requested.trim();
    if query.is_empty() {
        return Err(ToolError::Tool("Agent name must not be empty".to_string()));
    }

    if available_agents.is_empty() {
        return Err(ToolError::Tool(
            "No callable sub-agents available. Create an agent first.".to_string(),
        ));
    }

    if let Some(found) = available_agents.iter().find(|agent| agent.id == query) {
        return Ok(found.id.clone());
    }

    if let Some(found) = available_agents
        .iter()
        .find(|agent| agent.id.eq_ignore_ascii_case(query))
    {
        return Ok(found.id.clone());
    }

    let exact_name_matches: Vec<_> = available_agents
        .iter()
        .filter(|agent| agent.name.eq_ignore_ascii_case(query))
        .collect();
    if exact_name_matches.len() == 1 {
        return Ok(exact_name_matches[0].id.clone());
    }
    if exact_name_matches.len() > 1 {
        let ids = exact_name_matches
            .iter()
            .map(|agent| agent.id.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolError::Tool(format!(
            "Ambiguous agent name '{}'. Matching IDs: {}",
            query, ids
        )));
    }

    let normalized_query = normalize_identifier(query);
    let normalized_matches: Vec<_> = available_agents
        .iter()
        .filter(|agent| {
            normalize_identifier(&agent.id) == normalized_query
                || normalize_identifier(&agent.name) == normalized_query
        })
        .collect();
    if normalized_matches.len() == 1 {
        return Ok(normalized_matches[0].id.clone());
    }
    if normalized_matches.len() > 1 {
        let ids = normalized_matches
            .iter()
            .map(|agent| agent.id.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolError::Tool(format!(
            "Ambiguous agent identifier '{}'. Matching IDs: {}",
            query, ids
        )));
    }

    let suggestions = available_agents
        .iter()
        .take(8)
        .map(|agent| format!("{} ({})", agent.name, agent.id))
        .collect::<Vec<_>>()
        .join(", ");
    Err(ToolError::Tool(format!(
        "Unknown agent '{}'. Available agents: {}",
        query, suggestions
    )))
}

pub fn spawn_request_from_contract(
    available_agents: &[SubagentDefSummary],
    request: ContractSubagentSpawnRequest,
) -> Result<SpawnRequest, ToolError> {
    let task = request.task.trim();
    if task.is_empty() {
        return Err(ToolError::Tool(
            "Single spawn requires non-empty 'task'.".to_string(),
        ));
    }

    let inline = normalize_inline_config(request.inline);
    let agent_id = match request.agent_id {
        Some(agent_id) => Some(resolve_agent_id(available_agents, &agent_id)?),
        None => None,
    };
    if agent_id.is_some() && inline.is_some() {
        return Err(ToolError::Tool(
            "Inline temporary-subagent fields cannot be combined with 'agent'.".to_string(),
        ));
    }

    let (model, model_provider) =
        normalize_model_provider_pair(request.model, request.model_provider)?;

    Ok(SpawnRequest {
        agent_id,
        inline,
        task: task.to_string(),
        timeout_secs: request.timeout_secs,
        max_iterations: request.max_iterations,
        priority: request.priority.map(Into::into),
        model,
        model_provider,
        parent_execution_id: request.parent_execution_id,
        trace_session_id: request.trace_session_id,
        trace_scope_id: request.trace_scope_id,
        run_id: None,
    })
}

impl From<ContractSpawnPriority> for SpawnPriority {
    fn from(value: ContractSpawnPriority) -> Self {
        match value {
            ContractSpawnPriority::Low => Self::Low,
            ContractSpawnPriority::Normal => Self::Normal,
            ContractSpawnPriority::High => Self::High,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available_agents() -> Vec<SubagentDefSummary> {
        vec![
            SubagentDefSummary {
                id: "coder".to_string(),
                name: "Coder".to_string(),
                description: "Writes code".to_string(),
                tags: vec![],
            },
            SubagentDefSummary {
                id: "researcher".to_string(),
                name: "Research Agent".to_string(),
                description: "Researches".to_string(),
                tags: vec![],
            },
        ]
    }

    #[test]
    fn resolve_agent_id_matches_case_insensitive_name() {
        let resolved = resolve_agent_id(&available_agents(), "research agent")
            .expect("name lookup should resolve");
        assert_eq!(resolved, "researcher");
    }

    #[test]
    fn spawn_request_from_contract_rejects_model_provider_mismatch() {
        let error = spawn_request_from_contract(
            &available_agents(),
            ContractSubagentSpawnRequest {
                task: "write code".to_string(),
                model: Some("gpt-5.4-codex".to_string()),
                model_provider: None,
                ..ContractSubagentSpawnRequest::default()
            },
        )
        .expect_err("model/provider mismatch should fail");

        assert!(
            error
                .to_string()
                .contains("requires both 'model' and 'provider'")
        );
    }

    #[test]
    fn spawn_request_from_contract_rejects_agent_and_inline_combo() {
        let error = spawn_request_from_contract(
            &available_agents(),
            ContractSubagentSpawnRequest {
                agent_id: Some("coder".to_string()),
                inline: Some(ContractInlineSubagentConfig {
                    name: Some("Temp".to_string()),
                    ..ContractInlineSubagentConfig::default()
                }),
                task: "write code".to_string(),
                ..ContractSubagentSpawnRequest::default()
            },
        )
        .expect_err("agent plus inline should fail");

        assert!(
            error
                .to_string()
                .contains("cannot be combined with 'agent'")
        );
    }
}
