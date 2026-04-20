use anyhow::{Result, bail};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

use restflow_core::models::{ChatSession, ChatSessionSummary, ModelMetadataDTO};
use restflow_core::storage::agent::StoredAgent;
use restflow_traits::{TeamAssignment, TeamMessage, TeamState};

use super::daemon_client::TuiDaemonClient;
use super::event_loop::AppEvent;
use super::reducer::{ShellAction, ShellEffect};
use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand};
use super::state::{
    AppState, ModelPickerCategory, ModelPickerItem, OverlayState, ProviderPickerItem,
    RunPickerItem, TaskPickerItem, TeamPickerItem,
};
use super::transcript::ShellMessage;

#[derive(Clone)]
pub struct ShellController {
    client: TuiDaemonClient,
}

impl ShellController {
    pub fn new(client: TuiDaemonClient) -> Self {
        Self { client }
    }

    pub async fn daemon_running(&self) -> bool {
        self.client.daemon_running().await
    }

    pub async fn resolve_default_agent(
        &self,
        explicit: Option<&str>,
    ) -> Result<Option<StoredAgent>> {
        self.client.resolve_default_agent(explicit).await
    }

    pub async fn resolve_or_create_session(
        &self,
        agent: &StoredAgent,
        session_override: Option<&str>,
    ) -> Result<Option<ChatSession>> {
        self.client
            .resolve_or_create_session(agent, session_override)
            .await
    }

    pub fn spawn_session_events(
        &self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        self.client.spawn_session_events(tx)
    }

    pub fn spawn_task_events(
        &self,
        task_id: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        self.client.spawn_task_events(task_id, tx)
    }

    pub async fn execute_effect(
        &self,
        effect: ShellEffect,
        state: &AppState,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Vec<ShellAction>> {
        match effect {
            ShellEffect::RefreshState => self.refresh_actions(state).await,
            ShellEffect::ReloadCurrentSession => self.reload_current_session_actions(state).await,
            ShellEffect::ActivateOverlaySelection => self.overlay_selection_actions(state).await,
            ShellEffect::CreateSessionForSubmit { message } => {
                self.create_session_for_submit_actions(state, message).await
            }
            ShellEffect::SubmitMessage { message } => {
                self.submit_message_effect(state, message, tx).await?;
                Ok(Vec::new())
            }
            ShellEffect::ExecuteSlashCommand(command) => {
                self.slash_command_actions(state, command).await
            }
            ShellEffect::DeleteSession { session_id } => {
                self.delete_session_actions(session_id).await
            }
            ShellEffect::ListSessionsInline => self.session_picker_actions().await,
            ShellEffect::ListTeamsInline => self.team_picker_actions(state).await,
            ShellEffect::ListRunsInline => self.list_runs_inline_actions(state).await,
            ShellEffect::ListApprovalsInline => Ok(self.list_approvals_inline_actions(state)),
            ShellEffect::ClearScreen => Ok(Vec::new()),
        }
    }

    async fn refresh_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let sessions: Vec<ChatSessionSummary> =
            self.client.list_sessions().await.unwrap_or_default();
        let runs = if let Some(session_id) = state.current_session_id() {
            self.client
                .list_runs_for_session(session_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut actions = vec![ShellAction::StateRefreshed { sessions, runs }];

        if let Some(team_run_id) = state
            .current_team_state
            .as_ref()
            .map(|team| team.team_run_id.clone())
        {
            actions.extend(self.load_team_actions(&team_run_id, false).await?);
        }

        Ok(actions)
    }

    async fn reload_current_session_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let Some(session_id) = state.current_session_id().map(ToOwned::to_owned) else {
            return self.refresh_actions(state).await;
        };

        let session = self.client.get_session(&session_id).await.ok();
        let runs = if session.is_some() {
            self.client
                .list_runs_for_session(&session_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut actions = vec![ShellAction::CurrentSessionReloaded {
            session: session.map(Box::new),
            runs,
        }];
        actions.extend(self.refresh_actions(state).await?);
        Ok(actions)
    }

    async fn start_daemon_actions(
        &self,
        explicit_agent: Option<&str>,
        session_override: Option<&str>,
    ) -> Result<Vec<ShellAction>> {
        self.client.start_daemon().await?;
        Ok(vec![
            self.build_daemon_started_action(explicit_agent, session_override)
                .await?,
        ])
    }

    async fn build_daemon_started_action(
        &self,
        explicit_agent: Option<&str>,
        session_override: Option<&str>,
    ) -> Result<ShellAction> {
        let agent = self.resolve_default_agent(explicit_agent).await?;
        let session = if let Some(agent) = agent.as_ref() {
            self.resolve_or_create_session(agent, session_override)
                .await?
        } else {
            None
        };

        let status = if agent.is_some() {
            "Connected to daemon".to_string()
        } else {
            "No default agent configured. Create one from the standard CLI.".to_string()
        };

        Ok(ShellAction::DaemonStarted {
            agent: agent.map(Box::new),
            session: session.map(Box::new),
            status,
        })
    }

    async fn overlay_selection_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        match state.overlay.clone() {
            Some(OverlayState::CommandPicker { .. }) => {
                let Some(index) = state.selected_command_index() else {
                    return Ok(Vec::new());
                };
                let Some(spec) = SLASH_COMMAND_SPECS.get(index) else {
                    return Ok(Vec::new());
                };
                if matches!(spec.command, "/daemon") {
                    return Ok(vec![ShellAction::OpenDaemonPicker]);
                }
                if matches!(spec.command, "/task") {
                    return self.task_picker_actions().await;
                }
                if matches!(spec.command, "/team") {
                    return self.team_picker_actions(state).await;
                }
                if matches!(spec.command, "/model") {
                    return self.provider_picker_actions(state).await;
                }
                let command = command_display(spec.command, spec.args);
                if spec.args.is_empty() {
                    return Ok(vec![ShellAction::SubmitText { text: command }]);
                }
                Ok(vec![ShellAction::CommandPicked {
                    text: format!("{command} "),
                }])
            }
            Some(OverlayState::DaemonPicker { .. }) => {
                let Some(action) = state.selected_daemon_action() else {
                    return Ok(Vec::new());
                };
                Ok(vec![ShellAction::SubmitText {
                    text: format!("/daemon {action}"),
                }])
            }
            Some(OverlayState::SessionPicker { .. }) => {
                let Some(session_id) = state.selected_session_id().map(str::to_string) else {
                    return Ok(Vec::new());
                };
                let session = self.client.get_session(&session_id).await?;
                let runs = self
                    .client
                    .list_runs_for_session(&session_id)
                    .await
                    .unwrap_or_default();
                Ok(vec![ShellAction::SessionOpened {
                    session: Box::new(session),
                    runs,
                    status: format!("Opened session {session_id}"),
                }])
            }
            Some(OverlayState::TaskPicker { .. }) => {
                let Some(task_id) = state.selected_task_id().map(str::to_string) else {
                    return Ok(Vec::new());
                };
                Ok(vec![ShellAction::OpenTaskActionPicker { task_id }])
            }
            Some(OverlayState::TaskActionPicker { .. }) => {
                let Some((task_id, action)) = state.selected_task_action() else {
                    return Ok(Vec::new());
                };
                Ok(vec![ShellAction::SubmitText {
                    text: format!("/task {action} {task_id}"),
                }])
            }
            Some(OverlayState::TeamPicker { .. }) => {
                let Some(item) = state.selected_team_item() else {
                    return Ok(Vec::new());
                };
                match item {
                    TeamPickerItem::Current { team_run_id, .. } => {
                        self.load_team_actions(&team_run_id, true).await
                    }
                    TeamPickerItem::Saved { name, .. } => Ok(vec![ShellAction::SubmitText {
                        text: format!("/team start {name}"),
                    }]),
                }
            }
            Some(OverlayState::ProviderPicker { .. }) => {
                let Some(item) = state.selected_provider_item() else {
                    return Ok(Vec::new());
                };
                self.model_picker_actions_for_provider(state, item.provider)
                    .await
            }
            Some(OverlayState::ModelPicker { .. }) => {
                let Some(item) = state.selected_model_item() else {
                    return Ok(Vec::new());
                };
                if state.current_session_id().is_none() {
                    return Ok(vec![ShellAction::PendingSessionModelSelected {
                        provider: item.provider,
                        model: item.model,
                        model_name: item.name,
                        status: "Model selected for new chat.".to_string(),
                    }]);
                }
                self.switch_model_actions(state, item.model).await
            }
            Some(OverlayState::RunPicker { .. }) => {
                let Some(RunPickerItem::Run { run_id, .. }) = state.selected_run_picker_item()
                else {
                    return Ok(Vec::new());
                };
                let thread = self.client.get_execution_run_thread(&run_id).await?;
                let child_runs = self
                    .client
                    .list_child_runs(&run_id)
                    .await
                    .unwrap_or_default();
                let session = if let Some(session_id) = thread.focus.session_id.as_deref() {
                    self.client.get_session(session_id).await.ok()
                } else {
                    None
                };
                Ok(vec![ShellAction::RunOpened {
                    session: session.map(Box::new),
                    run_id: run_id.clone(),
                    thread: Box::new(thread),
                    child_runs,
                    status: format!("Opened run {run_id}"),
                }])
            }
            Some(OverlayState::ApprovalPicker { .. }) => Ok(Vec::new()),
            Some(OverlayState::TeamView { .. }) | Some(OverlayState::Help) | None => Ok(Vec::new()),
        }
    }

    async fn submit_message_effect(
        &self,
        state: &AppState,
        message: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let session_id = match state.current_session_id() {
            Some(session_id) => session_id.to_string(),
            None => bail!("No active session available."),
        };
        self.client.spawn_chat_stream(session_id, message, tx);
        Ok(())
    }

    async fn create_session_for_submit_actions(
        &self,
        state: &AppState,
        message: String,
    ) -> Result<Vec<ShellAction>> {
        let agent_id = state
            .pending_session
            .as_ref()
            .map(|session| session.agent_id.as_str())
            .or(state.default_agent_id.as_deref());
        let Some(agent_id) = agent_id else {
            bail!("No default agent configured. Create one from the standard CLI.");
        };
        let session = self
            .client
            .create_session_for_agent(
                agent_id,
                state
                    .pending_session
                    .as_ref()
                    .map(|session| session.model.as_str()),
            )
            .await?;
        Ok(vec![ShellAction::SessionCreatedForSubmit {
            session: Box::new(session),
            runs: Vec::new(),
            message,
        }])
    }

    async fn slash_command_actions(
        &self,
        state: &AppState,
        command: SlashCommand,
    ) -> Result<Vec<ShellAction>> {
        match command {
            SlashCommand::Daemon => Ok(vec![ShellAction::OpenDaemonPicker]),
            SlashCommand::Start => {
                match self
                    .start_daemon_actions(
                        state
                            .startup_state()
                            .and_then(|startup| startup.agent_override.as_deref()),
                        state
                            .startup_state()
                            .and_then(|startup| startup.session_override.as_deref()),
                    )
                    .await
                {
                    Ok(actions) => Ok(actions),
                    Err(err) => Ok(start_daemon_error_actions(err)),
                }
            }
            SlashCommand::Stop => {
                let stopped = self.client.stop_daemon().await?;
                let status = if stopped {
                    "RestFlow daemon stopped".to_string()
                } else {
                    "RestFlow daemon was not running".to_string()
                };
                Ok(vec![ShellAction::DaemonStopped { status }])
            }
            SlashCommand::Help => Ok(vec![ShellAction::MessageAppended(
                ShellMessage::InfoNotice {
                    content: help_text().to_string(),
                },
            )]),
            SlashCommand::ListSessions => self.session_picker_actions().await,
            SlashCommand::ListTasks => self.task_picker_actions().await,
            SlashCommand::ListModels => self.provider_picker_actions(state).await,
            SlashCommand::ListModelsForProvider { provider } => {
                match self
                    .resolve_provider_for_model_command(state, &provider)
                    .await?
                {
                    ModelCommandTarget::Provider(provider) => {
                        self.model_picker_actions_for_provider(state, provider)
                            .await
                    }
                    ModelCommandTarget::Model(model) => {
                        self.switch_model_actions(state, model).await
                    }
                }
            }
            SlashCommand::ListRuns => self.list_runs_inline_actions(state).await,
            SlashCommand::ListApprovals => Ok(self.list_approvals_inline_actions(state)),
            SlashCommand::ListTeams => self.team_picker_actions(state).await,
            SlashCommand::SwitchModel { model } => self.switch_model_actions(state, model).await,
            SlashCommand::TaskControl { action, task_id } => {
                let task = self.client.control_task(&task_id, action.as_str()).await?;
                Ok(vec![ShellAction::TaskControlCompleted {
                    task_id: task.id,
                    status: format!("{:?}", task.status),
                }])
            }
            SlashCommand::OpenRun { run_id } => {
                let thread = self.client.get_execution_run_thread(&run_id).await?;
                let child_runs = self
                    .client
                    .list_child_runs(&run_id)
                    .await
                    .unwrap_or_default();
                let session = if let Some(session_id) = thread.focus.session_id.as_deref() {
                    self.client.get_session(session_id).await.ok()
                } else {
                    None
                };
                Ok(vec![ShellAction::RunOpened {
                    session: session.map(Box::new),
                    run_id: run_id.clone(),
                    thread: Box::new(thread),
                    child_runs,
                    status: format!("Opened run {run_id}"),
                }])
            }
            SlashCommand::TeamState { team_run_id } => {
                self.load_team_actions(&team_run_id, false).await
            }
            SlashCommand::TeamStart { saved_team } => {
                let output = self
                    .client
                    .execute_runtime_tool(
                        "manage_teams",
                        json!({
                            "operation": "start_team",
                            "team": saved_team,
                        }),
                    )
                    .await?;
                if !output.success {
                    bail!(
                        output
                            .error
                            .unwrap_or_else(|| "manage_teams failed".to_string())
                    );
                }
                let team_state = serde_json::from_value::<TeamState>(output.result["team"].clone())
                    .ok()
                    .ok_or_else(|| anyhow::anyhow!("start_team did not return team state"))?;
                let team_run_id = team_state.team_run_id.clone();
                let mut actions = vec![ShellAction::MessageAppended(ShellMessage::TeamNotice {
                    content: format!("Started team {team_run_id}"),
                })];
                actions.extend(self.load_team_actions(&team_run_id, false).await?);
                Ok(actions)
            }
            SlashCommand::Approve { approval_id } => {
                self.approve_named_approval_actions(state, &approval_id)
                    .await
            }
            SlashCommand::Reject {
                approval_id,
                reason,
            } => {
                self.reject_named_approval_actions(state, &approval_id, reason)
                    .await
            }
        }
    }

    async fn session_picker_actions(&self) -> Result<Vec<ShellAction>> {
        let sessions = self.client.list_sessions().await?;
        let bound_session_ids = self
            .client
            .list_background_bound_session_ids()
            .await
            .unwrap_or_default();
        let sessions = filter_resume_sessions(sessions, &bound_session_ids);
        let status = if sessions.is_empty() {
            "No sessions to resume yet.".to_string()
        } else {
            "Select a session to resume".to_string()
        };
        Ok(vec![ShellAction::SessionPickerLoaded { sessions, status }])
    }

    async fn task_picker_actions(&self) -> Result<Vec<ShellAction>> {
        let tasks = self.client.list_tasks().await?;
        let tasks = tasks
            .into_iter()
            .map(|task| TaskPickerItem {
                task_id: task.id,
                name: task.name,
                status: format!("{:?}", task.status),
                next_run_at: task.next_run_at,
            })
            .collect::<Vec<_>>();
        let status = if tasks.is_empty() {
            "No tasks available.".to_string()
        } else {
            "Select a task".to_string()
        };
        Ok(vec![ShellAction::TaskPickerLoaded { tasks, status }])
    }

    async fn team_picker_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let mut items = Vec::new();
        if let Some(team) = state.current_team_state.as_ref() {
            items.push(TeamPickerItem::Current {
                team_run_id: team.team_run_id.clone(),
                status: format!("{:?}", team.status),
                members: team.members.len(),
            });
        }

        if let Ok(output) = self
            .client
            .execute_runtime_tool("spawn_subagent_batch", json!({ "operation": "list_teams" }))
            .await
            && output.success
            && let Some(teams) = output
                .result
                .get("teams")
                .and_then(serde_json::Value::as_array)
        {
            for team in teams {
                let Some(name) = team.get("team").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                items.push(TeamPickerItem::Saved {
                    name: name.to_string(),
                    member_groups: team
                        .get("member_groups")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default() as usize,
                    total_instances: team
                        .get("total_instances")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default() as usize,
                });
            }
        }

        let status = if items.is_empty() {
            "No teams available.".to_string()
        } else {
            "Select a team".to_string()
        };
        Ok(vec![ShellAction::TeamPickerLoaded { items, status }])
    }

    async fn provider_picker_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let available = if state.available_models.is_empty() {
            self.client.list_available_models().await?
        } else {
            state.available_models.clone()
        };
        let sessions = if state.sessions.is_empty() {
            self.client.list_sessions().await.unwrap_or_default()
        } else {
            state.sessions.clone()
        };
        let items =
            build_provider_picker_items(&sessions, &available, state.current_model_identity());
        let status = if items.is_empty() {
            "No available providers. Configure provider credentials first.".to_string()
        } else {
            "Select a provider".to_string()
        };
        Ok(vec![ShellAction::ProviderPickerLoaded {
            items,
            available_models: available,
            sessions,
            status,
        }])
    }

    async fn model_picker_actions_for_provider(
        &self,
        state: &AppState,
        provider: String,
    ) -> Result<Vec<ShellAction>> {
        let available = if state.available_models.is_empty() {
            self.client.list_available_models().await?
        } else {
            state.available_models.clone()
        };
        let sessions = if state.sessions.is_empty() {
            self.client.list_sessions().await.unwrap_or_default()
        } else {
            state.sessions.clone()
        };
        let items = build_model_picker_items_for_provider(
            &sessions,
            &available,
            state.current_model_identity(),
            &provider,
        );
        let status = if items.is_empty() {
            format!("No available models for {provider}.")
        } else {
            format!("Select a {provider} model")
        };
        Ok(vec![ShellAction::ModelPickerLoaded {
            provider,
            items,
            status,
        }])
    }

    async fn switch_model_actions(
        &self,
        state: &AppState,
        model: String,
    ) -> Result<Vec<ShellAction>> {
        let Some(session_id) = state.current_session_id() else {
            let available = self
                .client
                .list_available_models()
                .await
                .unwrap_or_default();
            let Some(item) = resolve_model_picker_item(&available, &model) else {
                return Ok(vec![ShellAction::StatusUpdated(format!(
                    "Unknown or unavailable model: {model}"
                ))]);
            };
            return Ok(vec![ShellAction::PendingSessionModelSelected {
                provider: item.provider,
                model: item.model,
                model_name: item.name,
                status: "Model selected for new chat.".to_string(),
            }]);
        };
        match self.client.update_session_model(session_id, &model).await {
            Ok(session) => Ok(vec![ShellAction::ModelSwitched {
                session: Box::new(session),
                status: format!("Switched model to {model}"),
            }]),
            Err(error) => Ok(vec![ShellAction::StatusUpdated(format!(
                "Failed to switch model: {error}"
            ))]),
        }
    }

    async fn resolve_provider_for_model_command(
        &self,
        state: &AppState,
        value: &str,
    ) -> Result<ModelCommandTarget> {
        let available = self.client.list_available_models().await?;
        if available
            .iter()
            .any(|metadata| metadata.provider.as_canonical_str() == value)
        {
            return Ok(ModelCommandTarget::Provider(value.to_string()));
        }
        let Some(item) = resolve_model_picker_item(&available, value) else {
            return Ok(ModelCommandTarget::Provider(value.to_string()));
        };
        if state.current_session_id().is_none() {
            return Ok(ModelCommandTarget::Model(item.model));
        }
        Ok(ModelCommandTarget::Model(item.model))
    }

    async fn delete_session_actions(&self, session_id: String) -> Result<Vec<ShellAction>> {
        let delete_result = self.client.delete_session(&session_id).await;
        let sessions = self.client.list_sessions().await.unwrap_or_default();
        let bound_session_ids = self
            .client
            .list_background_bound_session_ids()
            .await
            .unwrap_or_default();
        let sessions = filter_resume_sessions(sessions, &bound_session_ids);
        let (deleted, status) = match delete_result {
            Ok(deleted) if deleted => (true, format!("Deleted session {session_id}")),
            Ok(_) => (false, format!("Session {session_id} was not deleted")),
            Err(error) => (
                false,
                delete_session_error_message(&session_id, error.to_string()),
            ),
        };
        Ok(vec![ShellAction::SessionDeleted {
            session_id,
            deleted,
            sessions,
            status,
        }])
    }

    async fn list_runs_inline_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let items = state.run_picker_items();
        if items.is_empty() {
            return Ok(vec![ShellAction::MessageAppended(
                ShellMessage::InfoNotice {
                    content: "No runs for the current session.".to_string(),
                },
            )]);
        }

        let mut lines = vec!["Runs".to_string()];
        for item in items {
            let RunPickerItem::Run {
                run_id,
                title,
                status,
            } = item;
            lines.push(format!("- {title} · {status} · {run_id}"));
        }
        lines.push("Open one with /run open <run_id>".to_string());
        Ok(vec![ShellAction::MessageAppended(
            ShellMessage::InfoNotice {
                content: lines.join("\n"),
            },
        )])
    }

    fn list_approvals_inline_actions(&self, state: &AppState) -> Vec<ShellAction> {
        if state.current_team_approvals.is_empty() {
            return vec![ShellAction::MessageAppended(ShellMessage::InfoNotice {
                content: "No pending approvals.".to_string(),
            })];
        }

        let mut lines = vec!["Approvals".to_string()];
        for approval in &state.current_team_approvals {
            lines.push(format!(
                "- {} · {} · #{}",
                approval.member_id, approval.content, approval.approval_id
            ));
        }
        lines.push("Use /approve <approval_id> or /reject <approval_id> [reason]".to_string());
        vec![ShellAction::MessageAppended(ShellMessage::InfoNotice {
            content: lines.join("\n"),
        })]
    }

    async fn approve_named_approval_actions(
        &self,
        state: &AppState,
        approval_id: &str,
    ) -> Result<Vec<ShellAction>> {
        let team_run_id = state
            .current_team_state
            .as_ref()
            .map(|team| team.team_run_id.clone())
            .ok_or_else(|| anyhow::anyhow!("No active team context for approval"))?;
        if approval_id.trim().is_empty() {
            bail!("Usage: /approve <approval_id>");
        }
        let output = self
            .client
            .execute_runtime_tool(
                "manage_teams",
                json!({
                    "operation": "resolve_team_approval",
                    "team_run_id": team_run_id,
                    "approval_id": approval_id,
                    "approved": true,
                }),
            )
            .await?;
        if !output.success {
            bail!(
                output
                    .error
                    .unwrap_or_else(|| "approval failed".to_string())
            );
        }

        let mut actions = self.load_team_actions(&team_run_id, false).await?;
        actions.push(ShellAction::MessageAppended(ShellMessage::TeamNotice {
            content: format!("Approval {approval_id} approved"),
        }));
        actions.push(ShellAction::StatusUpdated(format!(
            "Approved {approval_id}"
        )));
        Ok(actions)
    }

    async fn reject_named_approval_actions(
        &self,
        state: &AppState,
        approval_id: &str,
        reason: Option<String>,
    ) -> Result<Vec<ShellAction>> {
        let team_run_id = state
            .current_team_state
            .as_ref()
            .map(|team| team.team_run_id.clone())
            .ok_or_else(|| anyhow::anyhow!("No active team context for rejection"))?;
        if approval_id.trim().is_empty() {
            bail!("Usage: /reject <approval_id> [reason]");
        }
        let output = self
            .client
            .execute_runtime_tool(
                "manage_teams",
                json!({
                    "operation": "resolve_team_approval",
                    "team_run_id": team_run_id,
                    "approval_id": approval_id,
                    "approved": false,
                    "reason": reason,
                }),
            )
            .await?;
        if !output.success {
            bail!(output.error.unwrap_or_else(|| "reject failed".to_string()));
        }

        let mut actions = self.load_team_actions(&team_run_id, false).await?;
        actions.push(ShellAction::MessageAppended(ShellMessage::TeamNotice {
            content: format!("Approval {approval_id} rejected"),
        }));
        actions.push(ShellAction::StatusUpdated(format!(
            "Rejected {approval_id}"
        )));
        Ok(actions)
    }

    async fn load_team_actions(
        &self,
        team_run_id: &str,
        open_overlay: bool,
    ) -> Result<Vec<ShellAction>> {
        let state_result = self
            .client
            .execute_runtime_tool(
                "manage_teams",
                json!({
                    "operation": "get_team_state",
                    "team_run_id": team_run_id,
                }),
            )
            .await?;
        if !state_result.success {
            bail!(
                "{}",
                state_result
                    .error
                    .unwrap_or_else(|| "get_team_state failed".to_string())
            );
        }
        let team_state = serde_json::from_value(state_result.result["team"].clone()).ok();

        let messages_result = self
            .client
            .execute_runtime_tool(
                "manage_teams",
                json!({
                    "operation": "list_team_messages",
                    "team_run_id": team_run_id,
                }),
            )
            .await?;
        let messages: Vec<TeamMessage> = if messages_result.success {
            serde_json::from_value(messages_result.result["messages"].clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        let assignments_result = self
            .client
            .execute_runtime_tool(
                "manage_teams",
                json!({
                    "operation": "list_team_assignments",
                    "team_run_id": team_run_id,
                }),
            )
            .await?;
        let assignments: Vec<TeamAssignment> = if assignments_result.success {
            serde_json::from_value(assignments_result.result["assignments"].clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(vec![ShellAction::TeamSnapshotLoaded {
            team_state,
            messages,
            assignments,
            status: format!("Loaded team {team_run_id}"),
            open_overlay,
        }])
    }
}

fn filter_resume_sessions(
    sessions: Vec<ChatSessionSummary>,
    bound_session_ids: &std::collections::HashSet<String>,
) -> Vec<ChatSessionSummary> {
    sessions
        .into_iter()
        .filter(|session| session.message_count > 0 && !bound_session_ids.contains(&session.id))
        .collect()
}

#[derive(Debug, Clone, Default)]
struct ModelUsage {
    count: usize,
    last_used_at: Option<i64>,
}

enum ModelCommandTarget {
    Provider(String),
    Model(String),
}

fn model_key(provider: &str, model: &str) -> String {
    format!("{}:{}", provider.trim(), model.trim())
}

fn model_usage_by_key(sessions: &[ChatSessionSummary]) -> HashMap<String, ModelUsage> {
    let mut usage = HashMap::<String, ModelUsage>::new();
    for session in sessions {
        if session.provider.trim().is_empty() || session.model.trim().is_empty() {
            continue;
        }
        let entry = usage
            .entry(model_key(&session.provider, &session.model))
            .or_default();
        entry.count += 1;
        entry.last_used_at = Some(
            entry
                .last_used_at
                .map(|existing| existing.max(session.updated_at))
                .unwrap_or(session.updated_at),
        );
    }
    usage
}

fn provider_usage_by_key(sessions: &[ChatSessionSummary]) -> HashMap<String, ModelUsage> {
    let mut usage = HashMap::<String, ModelUsage>::new();
    for session in sessions {
        if session.provider.trim().is_empty() {
            continue;
        }
        let entry = usage.entry(session.provider.clone()).or_default();
        entry.count += 1;
        entry.last_used_at = Some(
            entry
                .last_used_at
                .map(|existing| existing.max(session.updated_at))
                .unwrap_or(session.updated_at),
        );
    }
    usage
}

fn recent_usage_keys(usage: &HashMap<String, ModelUsage>, limit: usize) -> HashSet<String> {
    let mut entries = usage
        .iter()
        .filter_map(|(key, usage)| usage.last_used_at.map(|last| (key.clone(), last)))
        .collect::<Vec<_>>();
    entries.sort_by_key(|(_, last)| std::cmp::Reverse(*last));
    entries
        .into_iter()
        .take(limit)
        .map(|(key, _)| key)
        .collect()
}

fn build_provider_picker_items(
    sessions: &[ChatSessionSummary],
    available: &[ModelMetadataDTO],
    current_model: Option<(&str, &str)>,
) -> Vec<ProviderPickerItem> {
    let usage = provider_usage_by_key(sessions);
    let recent_keys = recent_usage_keys(&usage, 5);
    let current_provider = current_model.map(|(provider, _)| provider.to_string());
    let mut providers = HashSet::<String>::new();

    let mut items = Vec::new();
    for metadata in available {
        let provider = metadata.provider.as_canonical_str().to_string();
        if !providers.insert(provider.clone()) {
            continue;
        }
        let usage = usage.get(&provider).cloned().unwrap_or_default();
        let category = if recent_keys.contains(&provider) {
            ModelPickerCategory::Recent
        } else if usage.count > 0 {
            ModelPickerCategory::Frequent
        } else {
            ModelPickerCategory::Available
        };
        items.push(ProviderPickerItem {
            label: provider.clone(),
            is_current: current_provider.as_deref() == Some(provider.as_str()),
            provider,
            category,
            usage_count: usage.count,
            last_used_at: usage.last_used_at,
        });
    }

    items.sort_by(|left, right| {
        model_category_order(left.category)
            .cmp(&model_category_order(right.category))
            .then_with(|| match left.category {
                ModelPickerCategory::Recent => right.last_used_at.cmp(&left.last_used_at),
                ModelPickerCategory::Frequent => right.usage_count.cmp(&left.usage_count),
                ModelPickerCategory::Available => std::cmp::Ordering::Equal,
            })
            .then_with(|| left.label.cmp(&right.label))
    });
    items
}

fn build_model_picker_items_for_provider(
    sessions: &[ChatSessionSummary],
    available: &[ModelMetadataDTO],
    current_model: Option<(&str, &str)>,
    provider_filter: &str,
) -> Vec<ModelPickerItem> {
    let usage = model_usage_by_key(sessions);
    let recent_keys = recent_usage_keys(&usage, 5);

    let current_key = current_model
        .map(|(provider, model)| model_key(provider, model))
        .filter(|key| key != ":");

    let mut items = available
        .iter()
        .filter(|metadata| metadata.provider.as_canonical_str() == provider_filter)
        .map(|metadata| {
            let provider = metadata.provider.as_canonical_str().to_string();
            let model = metadata.model.as_serialized_str().to_string();
            let key = model_key(&provider, &model);
            let usage = usage.get(&key).cloned().unwrap_or_default();
            let category = if recent_keys.contains(&key) {
                ModelPickerCategory::Recent
            } else if usage.count > 0 {
                ModelPickerCategory::Frequent
            } else {
                ModelPickerCategory::Available
            };
            ModelPickerItem {
                provider,
                model,
                name: metadata.name.clone(),
                category,
                usage_count: usage.count,
                last_used_at: usage.last_used_at,
                is_current: current_key.as_deref() == Some(key.as_str()),
            }
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        model_category_order(left.category)
            .cmp(&model_category_order(right.category))
            .then_with(|| match left.category {
                ModelPickerCategory::Recent => right.last_used_at.cmp(&left.last_used_at),
                ModelPickerCategory::Frequent => right.usage_count.cmp(&left.usage_count),
                ModelPickerCategory::Available => std::cmp::Ordering::Equal,
            })
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.model.cmp(&right.model))
    });
    items
}

fn resolve_model_picker_item(
    available: &[ModelMetadataDTO],
    requested: &str,
) -> Option<ModelPickerItem> {
    let requested = requested.trim();
    available.iter().find_map(|metadata| {
        let provider = metadata.provider.as_canonical_str().to_string();
        let model = metadata.model.as_serialized_str().to_string();
        let qualified = model_key(&provider, &model);
        if requested == model || requested == qualified {
            Some(ModelPickerItem {
                provider,
                model,
                name: metadata.name.clone(),
                category: ModelPickerCategory::Available,
                usage_count: 0,
                last_used_at: None,
                is_current: false,
            })
        } else {
            None
        }
    })
}

fn model_category_order(category: ModelPickerCategory) -> u8 {
    match category {
        ModelPickerCategory::Recent => 0,
        ModelPickerCategory::Frequent => 1,
        ModelPickerCategory::Available => 2,
    }
}

fn delete_session_error_message(session_id: &str, error: String) -> String {
    if error.contains("bound to background task") {
        format!("Cannot delete background-bound session {session_id}")
    } else {
        format!("Failed to delete session {session_id}: {error}")
    }
}

fn start_daemon_error_actions(err: anyhow::Error) -> Vec<ShellAction> {
    vec![ShellAction::Error(format!("Failed to start daemon: {err}"))]
}

fn command_display(command: &str, args: &str) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    }
}

fn help_text() -> &'static str {
    "RestFlow terminal shell\n\n\
Use /daemon when the daemon is offline.\n\
\
Enter sends the current draft.\n\
Ctrl-J inserts a newline.\n\
Ctrl-P resumes a previous session.\n\
Ctrl-A lists pending approvals.\n\
Ctrl-G shows current team state.\n\
Ctrl-L clears and redraws the screen.\n\
Ctrl-C exits.\n\n\
Slash commands:\n\
/daemon\n\
/help\n\
/resume\n\
/model\n\
/team\n\
/task"
}

#[cfg(test)]
mod tests {
    use super::{
        build_model_picker_items_for_provider, build_provider_picker_items,
        delete_session_error_message, filter_resume_sessions, start_daemon_error_actions,
    };
    use crate::reducer::ShellAction;
    use crate::state::ModelPickerCategory;
    use restflow_core::models::{ChatSessionSummary, ModelId, ModelMetadataDTO};
    use std::collections::HashSet;

    #[test]
    fn start_daemon_error_stays_inside_shell() {
        let actions = start_daemon_error_actions(anyhow::anyhow!("socket denied"));

        assert!(matches!(
            actions.as_slice(),
            [ShellAction::Error(message)]
                if message.contains("Failed to start daemon") && message.contains("socket denied")
        ));
    }

    #[test]
    fn filter_resume_sessions_removes_background_bound_sessions() {
        let sessions = vec![
            session_summary_with_messages("session-1", "Regular", 1),
            session_summary_with_messages("session-2", "Background", 1),
            session_summary_with_messages("session-3", "Empty", 0),
        ];
        let bound = HashSet::from(["session-2".to_string()]);

        let visible = filter_resume_sessions(sessions, &bound);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "session-1");
    }

    #[test]
    fn delete_session_error_message_summarizes_background_bound_conflict() {
        let message = delete_session_error_message(
            "session-1",
            "IPC error 409: Session session-1 is bound to background task task-1".to_string(),
        );

        assert_eq!(message, "Cannot delete background-bound session session-1");
    }

    #[test]
    fn provider_picker_orders_recent_frequent_then_available_providers() {
        let sessions = vec![
            session_summary_with_model("session-1", "Recent", "codex", "gpt-5.4", 100),
            session_summary_with_model(
                "session-2",
                "Frequent 1",
                "minimax-coding-plan",
                "minimax-coding-plan-m2-5",
                50,
            ),
            session_summary_with_model(
                "session-3",
                "Frequent 2",
                "minimax-coding-plan",
                "minimax-coding-plan-m2-5",
                60,
            ),
        ];
        let available = vec![
            model_metadata(ModelId::MiniMaxM25CodingPlan, "MiniMax M2.5"),
            model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4"),
            model_metadata(ModelId::Gpt5_4MiniCodex, "GPT-5.4 Mini"),
        ];
        let items = build_provider_picker_items(&sessions, &available, Some(("codex", "gpt-5.4")));

        assert_eq!(items[0].provider, "codex");
        assert_eq!(items[0].category, ModelPickerCategory::Recent);
        assert!(items[0].is_current);
        assert_eq!(items[1].provider, "minimax-coding-plan");
        assert_eq!(items[1].usage_count, 2);
    }

    #[test]
    fn model_picker_filters_models_to_selected_provider() {
        let sessions = vec![
            session_summary_with_model("session-1", "Recent", "codex", "gpt-5.4", 100),
            session_summary_with_model(
                "session-2",
                "Frequent 1",
                "minimax-coding-plan",
                "minimax-coding-plan-m2-5",
                50,
            ),
            session_summary_with_model(
                "session-3",
                "Frequent 2",
                "minimax-coding-plan",
                "minimax-coding-plan-m2-5",
                60,
            ),
        ];
        let available = vec![
            model_metadata(ModelId::MiniMaxM25CodingPlan, "MiniMax M2.5"),
            model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4"),
            model_metadata(ModelId::Gpt5_4MiniCodex, "GPT-5.4 Mini"),
        ];
        let items = build_model_picker_items_for_provider(
            &sessions,
            &available,
            Some(("codex", "gpt-5.4")),
            "codex",
        );

        assert_eq!(items[0].model, "gpt-5.4");
        assert_eq!(items[0].category, ModelPickerCategory::Recent);
        assert!(items[0].is_current);
        assert_eq!(items[1].model, "gpt-5.4-mini");
        assert_eq!(items[1].category, ModelPickerCategory::Available);
    }

    fn session_summary_with_messages(
        id: &str,
        name: &str,
        message_count: u32,
    ) -> ChatSessionSummary {
        ChatSessionSummary {
            id: id.to_string(),
            name: name.to_string(),
            agent_id: "agent-1".to_string(),
            provider: "provider".to_string(),
            model: "model".to_string(),
            skill_id: None,
            message_count,
            updated_at: 1,
            last_message_preview: Some("preview".to_string()),
            source_channel: None,
            source_conversation_id: None,
            archived_at: None,
        }
    }

    fn session_summary_with_model(
        id: &str,
        name: &str,
        provider: &str,
        model: &str,
        updated_at: i64,
    ) -> ChatSessionSummary {
        ChatSessionSummary {
            id: id.to_string(),
            name: name.to_string(),
            agent_id: "agent-1".to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            skill_id: None,
            message_count: 1,
            updated_at,
            last_message_preview: Some("preview".to_string()),
            source_channel: None,
            source_conversation_id: None,
            archived_at: None,
        }
    }

    fn model_metadata(model: ModelId, name: &str) -> ModelMetadataDTO {
        ModelMetadataDTO {
            model,
            provider: model.provider(),
            supports_temperature: false,
            name: name.to_string(),
        }
    }
}
