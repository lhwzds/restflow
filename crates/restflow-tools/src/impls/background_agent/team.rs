use serde_json::{Value, json};

use crate::Result;
use crate::ToolError;
use crate::impls::team_template::{
    TeamTemplateScope, delete_scoped_team_document, list_scoped_team_entries,
    load_scoped_team_document, save_scoped_team_document,
};
use restflow_traits::TeamTemplateDocument;
use restflow_traits::store::KvStore;

use super::types::{BackgroundBatchWorkerSpec, StoredBackgroundBatchWorkerSpec};

const BACKGROUND_AGENT_TEAM_SCOPE: TeamTemplateScope =
    TeamTemplateScope::new("background_agent_team", "background_agent_team", 2);

fn normalize_structural_worker(
    worker: &BackgroundBatchWorkerSpec,
    worker_index: usize,
    strict_runtime_inputs: bool,
) -> Result<StoredBackgroundBatchWorkerSpec> {
    if strict_runtime_inputs && (worker.input.is_some() || worker.inputs.is_some()) {
        return Err(ToolError::Tool(format!(
            "save_team stores worker structure only. Remove 'input'/'inputs' from worker index {} and pass runtime input during run_batch.",
            worker_index
        )));
    }
    if worker.count == 0 {
        return Err(ToolError::Tool(format!(
            "Worker index {} count must be >= 1.",
            worker_index
        )));
    }
    Ok(StoredBackgroundBatchWorkerSpec {
        agent_id: worker.agent_id.clone(),
        name: worker.name.clone(),
        count: worker
            .inputs
            .as_ref()
            .map(|items| items.len() as u32)
            .unwrap_or(worker.count),
        chat_session_id: worker.chat_session_id.clone(),
        schedule: worker.schedule.clone(),
        timeout_secs: worker.timeout_secs,
        durability_mode: worker.durability_mode.clone(),
        memory: worker.memory.clone(),
        memory_scope: worker.memory_scope.clone(),
        resource_limits: worker.resource_limits.clone(),
    })
}

fn runtime_worker_from_stored(
    worker: StoredBackgroundBatchWorkerSpec,
) -> BackgroundBatchWorkerSpec {
    BackgroundBatchWorkerSpec {
        agent_id: worker.agent_id,
        name: worker.name,
        input: None,
        inputs: None,
        count: worker.count,
        chat_session_id: worker.chat_session_id,
        schedule: worker.schedule,
        timeout_secs: worker.timeout_secs,
        durability_mode: worker.durability_mode,
        memory: worker.memory,
        memory_scope: worker.memory_scope,
        resource_limits: worker.resource_limits,
    }
}

pub(super) fn save_team_workers(
    store: &dyn KvStore,
    team_name: &str,
    workers: &[BackgroundBatchWorkerSpec],
    strict_runtime_inputs: bool,
) -> Result<Value> {
    if workers.is_empty() {
        return Err(ToolError::Tool(
            "save_team requires non-empty 'workers'.".to_string(),
        ));
    }
    let members = workers
        .iter()
        .enumerate()
        .map(|(worker_index, worker)| {
            normalize_structural_worker(worker, worker_index, strict_runtime_inputs)
        })
        .collect::<Result<Vec<_>>>()?;
    let persisted = save_scoped_team_document(
        store,
        BACKGROUND_AGENT_TEAM_SCOPE,
        team_name,
        members,
        Some(vec!["background_agent".to_string(), "team".to_string()]),
    )?;
    Ok(json!({
        "team": persisted.document.name,
        "workers": workers.len(),
        "created_at": persisted.document.created_at,
        "updated_at": persisted.document.updated_at,
        "storage": persisted.storage
    }))
}

pub(super) fn load_team_workers(
    store: &dyn KvStore,
    team_name: &str,
) -> Result<Vec<BackgroundBatchWorkerSpec>> {
    let team: TeamTemplateDocument<StoredBackgroundBatchWorkerSpec> =
        load_scoped_team_document(store, BACKGROUND_AGENT_TEAM_SCOPE, team_name)?;
    Ok(team
        .members
        .into_iter()
        .map(runtime_worker_from_stored)
        .collect())
}

pub(super) fn delete_team(store: &dyn KvStore, team_name: &str) -> Result<Value> {
    delete_scoped_team_document(store, BACKGROUND_AGENT_TEAM_SCOPE, team_name)
}

pub(super) fn get_team(store: &dyn KvStore, team_name: &str) -> Result<Value> {
    let document: TeamTemplateDocument<StoredBackgroundBatchWorkerSpec> =
        load_scoped_team_document(store, BACKGROUND_AGENT_TEAM_SCOPE, team_name)?;
    let members = document
        .members
        .clone()
        .into_iter()
        .map(runtime_worker_from_stored)
        .collect::<Vec<_>>();
    Ok(json!({
        "team": document.name,
        "version": document.version,
        "created_at": document.created_at,
        "updated_at": document.updated_at,
        "member_groups": members.len(),
        "total_instances": document.members.iter().map(|worker| worker.count as usize).sum::<usize>(),
        "members": members
    }))
}

pub(super) fn list_teams(store: &dyn KvStore) -> Result<Value> {
    let mut teams = Vec::new();
    for item in list_scoped_team_entries(store, BACKGROUND_AGENT_TEAM_SCOPE)? {
        let Some(team_name) = BACKGROUND_AGENT_TEAM_SCOPE.team_name_from_entry(&item) else {
            continue;
        };
        let (member_groups, total_instances, updated_at) =
            load_scoped_team_document::<StoredBackgroundBatchWorkerSpec>(
                store,
                BACKGROUND_AGENT_TEAM_SCOPE,
                &team_name,
            )
            .ok()
            .map(|team| {
                let total_instances = team
                    .members
                    .iter()
                    .map(|worker| worker.count as usize)
                    .sum::<usize>();
                (team.members.len(), total_instances, team.updated_at)
            })
            .unwrap_or((0, 0, 0));
        teams.push(json!({
            "team": team_name,
            "member_groups": member_groups,
            "total_instances": total_instances,
            "updated_at": updated_at
        }));
    }
    teams.sort_by(|left, right| {
        right["updated_at"]
            .as_i64()
            .unwrap_or_default()
            .cmp(&left["updated_at"].as_i64().unwrap_or_default())
            .then_with(|| {
                left["team"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["team"].as_str().unwrap_or_default())
            })
    });
    Ok(Value::Array(teams))
}
