use crate::{Result, ToolError};
use restflow_traits::InlineSubagentConfig;

use super::SpawnSubagentBatchTool;
use super::types::BatchSubagentSpec;

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

pub(super) fn resolve_agent_id(tool: &SpawnSubagentBatchTool, requested: &str) -> Result<String> {
    let query = requested.trim();
    if query.is_empty() {
        return Err(ToolError::Tool("Agent name must not be empty".to_string()));
    }

    let available = tool.available_agents();
    if available.is_empty() {
        return Err(ToolError::Tool(
            "No callable sub-agents available. Create an agent first.".to_string(),
        ));
    }

    if let Some(found) = available.iter().find(|agent| agent.id == query) {
        return Ok(found.id.clone());
    }
    if let Some(found) = available
        .iter()
        .find(|agent| agent.id.eq_ignore_ascii_case(query))
    {
        return Ok(found.id.clone());
    }

    let exact_name_matches: Vec<_> = available
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
    let normalized_matches: Vec<_> = available
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

    let suggestions = available
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

pub(super) fn build_inline_config(spec: &BatchSubagentSpec) -> Option<InlineSubagentConfig> {
    let config = InlineSubagentConfig {
        name: spec.inline_name.clone(),
        system_prompt: spec.inline_system_prompt.clone(),
        allowed_tools: spec.inline_allowed_tools.clone(),
        max_iterations: spec.inline_max_iterations,
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
