use serde_json::{Value, json};
use std::sync::Arc;

use crate::impls::team_template::{
    TeamTemplateScope, delete_scoped_team_document, list_scoped_team_entries,
    load_scoped_team_document, save_scoped_team_document,
};
use crate::{Result, ToolError, ToolOutput};
use restflow_traits::TeamTemplateDocument;
use restflow_traits::store::KvStore;

use super::SpawnSubagentBatchTool;
use super::types::{BatchSubagentSpec, StoredBatchSubagentSpec};
use super::validate::total_instances;

const SUBAGENT_TEAM_SCOPE: TeamTemplateScope =
    TeamTemplateScope::new("subagent_team", "subagent_team", 1);

pub(super) fn team_store(tool: &SpawnSubagentBatchTool) -> Result<Arc<dyn KvStore>> {
    tool.kv_store.clone().ok_or_else(|| {
        ToolError::Tool(
            "Team storage is unavailable in this runtime. Provide specs directly.".to_string(),
        )
    })
}

pub(super) fn structural_count(spec: &BatchSubagentSpec, spec_index: usize) -> Result<u32> {
    if let Some(tasks) = &spec.tasks {
        return u32::try_from(tasks.len()).map_err(|_| {
            ToolError::Tool(format!(
                "Spec index {} has too many tasks to store as a team member count.",
                spec_index
            ))
        });
    }
    Ok(spec.count)
}

pub(super) fn stored_spec_from_batch(
    spec: &BatchSubagentSpec,
    spec_index: usize,
) -> Result<StoredBatchSubagentSpec> {
    Ok(StoredBatchSubagentSpec {
        agent: spec.agent.clone(),
        count: structural_count(spec, spec_index)?,
        timeout_secs: spec.timeout_secs,
        model: spec.model.clone(),
        provider: spec.provider.clone(),
        inline_name: spec.inline_name.clone(),
        inline_system_prompt: spec.inline_system_prompt.clone(),
        inline_allowed_tools: spec.inline_allowed_tools.clone(),
        inline_max_iterations: spec.inline_max_iterations,
    })
}

pub(super) fn batch_spec_from_stored_value(
    value: Value,
    spec_index: usize,
) -> Result<BatchSubagentSpec> {
    let spec: BatchSubagentSpec = serde_json::from_value(value).map_err(|err| {
        ToolError::Tool(format!(
            "Stored team has invalid spec at index {}: {}",
            spec_index, err
        ))
    })?;
    let count = structural_count(&spec, spec_index)?;
    Ok(BatchSubagentSpec {
        agent: spec.agent,
        count,
        task: None,
        tasks: None,
        timeout_secs: spec.timeout_secs,
        model: spec.model,
        provider: spec.provider,
        inline_name: spec.inline_name,
        inline_system_prompt: spec.inline_system_prompt,
        inline_allowed_tools: spec.inline_allowed_tools,
        inline_max_iterations: spec.inline_max_iterations,
    })
}

pub(super) fn load_team_specs(
    tool: &SpawnSubagentBatchTool,
    team_name: &str,
) -> Result<Vec<BatchSubagentSpec>> {
    let store = team_store(tool)?;
    let team: TeamTemplateDocument<StoredBatchSubagentSpec> =
        load_scoped_team_document(store.as_ref(), SUBAGENT_TEAM_SCOPE, team_name)?;
    if team.members.is_empty() {
        return Err(ToolError::Tool(format!(
            "Team '{}' has no member specs.",
            team_name
        )));
    }
    team.members
        .into_iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|err| {
            ToolError::Tool(format!(
                "Stored team '{}' has invalid structural members: {}",
                team_name, err
            ))
        })?
        .into_iter()
        .enumerate()
        .map(|(spec_index, value)| batch_spec_from_stored_value(value, spec_index))
        .collect()
}

pub(super) fn save_team_specs(
    tool: &SpawnSubagentBatchTool,
    team_name: &str,
    specs: &[BatchSubagentSpec],
) -> Result<Value> {
    if specs.is_empty() {
        return Err(ToolError::Tool(
            "Cannot save team with empty specs.".to_string(),
        ));
    }
    let store = team_store(tool)?;
    let stored_specs = specs
        .iter()
        .enumerate()
        .map(|(spec_index, spec)| stored_spec_from_batch(spec, spec_index))
        .collect::<Result<Vec<_>>>()?;
    let persisted = save_scoped_team_document(
        store.as_ref(),
        SUBAGENT_TEAM_SCOPE,
        team_name,
        stored_specs,
        Some(vec!["subagent".to_string(), "team".to_string()]),
    )?;
    let total = total_instances(specs)?;

    Ok(json!({
        "saved": true,
        "team": persisted.document.name,
        "member_groups": specs.len(),
        "total_instances": total,
        "storage": persisted.storage
    }))
}

pub(super) fn list_teams(tool: &SpawnSubagentBatchTool) -> Result<ToolOutput> {
    let store = team_store(tool)?;
    let entries = list_scoped_team_entries(store.as_ref(), SUBAGENT_TEAM_SCOPE)?;

    let teams = entries
        .iter()
        .filter_map(|entry| {
            let team = SUBAGENT_TEAM_SCOPE.team_name_from_entry(entry)?;
            Some(json!({
                "team": team,
                "updated_at": entry.get("updated_at").cloned().unwrap_or(Value::Null),
                "tags": entry.get("tags").cloned().unwrap_or(Value::Null),
            }))
        })
        .collect::<Vec<_>>();

    Ok(ToolOutput::success(json!({
        "operation": "list_teams",
        "count": teams.len(),
        "teams": teams
    })))
}

pub(super) fn get_team(tool: &SpawnSubagentBatchTool, team_name: &str) -> Result<ToolOutput> {
    let store = team_store(tool)?;
    let team: TeamTemplateDocument<StoredBatchSubagentSpec> =
        load_scoped_team_document(store.as_ref(), SUBAGENT_TEAM_SCOPE, team_name)?;
    let specs = team
        .members
        .into_iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|err| ToolError::Tool(format!("Failed to parse stored team: {}", err)))?
        .into_iter()
        .enumerate()
        .map(|(spec_index, value)| batch_spec_from_stored_value(value, spec_index))
        .collect::<Result<Vec<_>>>()?;
    Ok(ToolOutput::success(json!({
        "operation": "get_team",
        "team": team.name,
        "version": team.version,
        "created_at": team.created_at,
        "updated_at": team.updated_at,
        "member_groups": specs.len(),
        "total_instances": total_instances(&specs)?,
        "members": specs
    })))
}

pub(super) fn delete_team(tool: &SpawnSubagentBatchTool, team_name: &str) -> Result<ToolOutput> {
    let store = team_store(tool)?;
    let deleted = delete_scoped_team_document(store.as_ref(), SUBAGENT_TEAM_SCOPE, team_name)?;
    Ok(ToolOutput::success(json!({
        "operation": "delete_team",
        "team": team_name,
        "result": deleted["result"].clone()
    })))
}
