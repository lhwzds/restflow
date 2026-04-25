use super::super::runtime::get_runtime_tool_registry;
use super::super::*;
use restflow_contracts::request::WireModelRef;
use serde_json::{Value, json};

impl IpcServer {
    pub(super) async fn handle_list_teams(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        include_saved: bool,
        include_active: bool,
    ) -> IpcResponse {
        let saved = if include_saved {
            match Self::execute_runtime_tool_json(
                core,
                runtime_tool_registry,
                "spawn_subagent_batch",
                json!({ "operation": "list_teams" }),
            )
            .await
            {
                Ok(value) => value.get("teams").cloned().unwrap_or_else(|| json!([])),
                Err(error) => return IpcResponse::error(500, error),
            }
        } else {
            json!([])
        };

        let active = if include_active {
            match Self::execute_runtime_tool_json(
                core,
                runtime_tool_registry,
                "manage_teams",
                json!({ "operation": "list_team_states" }),
            )
            .await
            {
                Ok(value) => value.get("teams").cloned().unwrap_or_else(|| json!([])),
                Err(error) => return IpcResponse::error(500, error),
            }
        } else {
            json!([])
        };

        IpcResponse::success(json!({
            "saved": saved,
            "active": active,
        }))
    }

    pub(super) async fn handle_get_team_snapshot(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        team_run_id: String,
    ) -> IpcResponse {
        match Self::team_snapshot_json(core, runtime_tool_registry, &team_run_id).await {
            Ok(snapshot) => IpcResponse::success(snapshot),
            Err(error) => IpcResponse::error(500, error),
        }
    }

    pub(super) async fn handle_start_team(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        team: String,
        assignments: Vec<String>,
    ) -> IpcResponse {
        if assignments.is_empty() {
            return IpcResponse::error(400, "start_team requires at least one assignment");
        }

        let result = Self::execute_runtime_tool_json(
            core,
            runtime_tool_registry,
            "manage_teams",
            json!({
                "operation": "start_team",
                "team": team,
                "assignments": assignments,
            }),
        )
        .await;
        let team = match result {
            Ok(value) => value.get("team").cloned().unwrap_or(Value::Null),
            Err(error) => return IpcResponse::error(500, error),
        };
        let Some(team_run_id) = team.get("team_run_id").and_then(Value::as_str) else {
            return IpcResponse::error(500, "start_team did not return a team_run_id");
        };

        match Self::team_snapshot_json(core, runtime_tool_registry, team_run_id).await {
            Ok(snapshot) => IpcResponse::success(snapshot),
            Err(error) => IpcResponse::error(500, error),
        }
    }

    pub(super) async fn handle_resolve_team_approval(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        team_run_id: String,
        approval_id: String,
        approved: bool,
        reason: Option<String>,
    ) -> IpcResponse {
        let result = Self::execute_runtime_tool_json(
            core,
            runtime_tool_registry,
            "manage_teams",
            json!({
                "operation": "resolve_team_approval",
                "team_run_id": team_run_id,
                "approval_id": approval_id,
                "approved": approved,
                "reason": reason,
            }),
        )
        .await;
        match result {
            Ok(value) => IpcResponse::success(value),
            Err(error) => IpcResponse::error(500, error),
        }
    }

    pub(super) async fn handle_list_run_artifacts(
        core: &Arc<AppCore>,
        run_id: Option<String>,
        task_id: Option<String>,
        team_run_id: Option<String>,
    ) -> IpcResponse {
        let result = if let Some(run_id) = run_id {
            core.storage.run_artifacts.list_by_run(&run_id)
        } else if let Some(task_id) = task_id {
            core.storage.run_artifacts.list_by_task(&task_id)
        } else if let Some(team_run_id) = team_run_id {
            core.storage.run_artifacts.list_by_team(&team_run_id)
        } else {
            return IpcResponse::error(
                400,
                "ListRunArtifacts requires run_id, task_id, or team_run_id",
            );
        };

        match result {
            Ok(artifacts) => IpcResponse::success(artifacts),
            Err(error) => IpcResponse::error(500, error.to_string()),
        }
    }

    pub(super) async fn handle_switch_session_model(
        core: &Arc<AppCore>,
        session_id: String,
        model_ref: WireModelRef,
    ) -> IpcResponse {
        let mut session = match core.storage.chat_sessions.get(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("session"),
            Err(error) => return IpcResponse::error(500, error.to_string()),
        };
        session.provider = model_ref.provider;
        session.model = model_ref.model;
        session.updated_at = chrono::Utc::now().timestamp_millis();

        match core.storage.chat_sessions.update(&session) {
            Ok(()) => match core.storage.chat_sessions.get(&session_id) {
                Ok(Some(session)) => IpcResponse::success(session),
                Ok(None) => IpcResponse::not_found("session"),
                Err(error) => IpcResponse::error(500, error.to_string()),
            },
            Err(error) => IpcResponse::error(500, error.to_string()),
        }
    }

    async fn team_snapshot_json(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        team_run_id: &str,
    ) -> Result<Value, String> {
        let state = Self::execute_runtime_tool_json(
            core,
            runtime_tool_registry,
            "manage_teams",
            json!({
                "operation": "get_team_state",
                "team_run_id": team_run_id,
            }),
        )
        .await?;
        let messages = Self::execute_runtime_tool_json(
            core,
            runtime_tool_registry,
            "manage_teams",
            json!({
                "operation": "list_team_messages",
                "team_run_id": team_run_id,
            }),
        )
        .await?;
        let assignments = Self::execute_runtime_tool_json(
            core,
            runtime_tool_registry,
            "manage_teams",
            json!({
                "operation": "list_team_assignments",
                "team_run_id": team_run_id,
            }),
        )
        .await?;
        let approvals = Self::execute_runtime_tool_json(
            core,
            runtime_tool_registry,
            "manage_teams",
            json!({
                "operation": "list_team_approvals",
                "team_run_id": team_run_id,
            }),
        )
        .await?;

        Ok(json!({
            "team": state.get("team").cloned().unwrap_or(Value::Null),
            "messages": messages.get("messages").cloned().unwrap_or_else(|| json!([])),
            "assignments": assignments
                .get("assignments")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "approvals": approvals.get("approvals").cloned().unwrap_or_else(|| json!([])),
        }))
    }

    async fn execute_runtime_tool_json(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        name: &str,
        input: Value,
    ) -> Result<Value, String> {
        let registry = get_runtime_tool_registry(core, runtime_tool_registry)?;
        let output = registry
            .execute_safe(name, input)
            .await
            .map_err(|error| error.to_string())?;
        if output.success {
            Ok(output.result)
        } else {
            Err(output
                .error
                .unwrap_or_else(|| format!("{name} execution failed")))
        }
    }
}
