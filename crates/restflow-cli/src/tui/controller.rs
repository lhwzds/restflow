use anyhow::{Result, bail};
use serde_json::json;
use tokio::sync::mpsc;

use restflow_core::models::{ChatSession, ChatSessionSummary};
use restflow_core::storage::agent::StoredAgent;
use restflow_traits::{TeamAssignment, TeamMessage, TeamState};

use super::daemon_client::TuiDaemonClient;
use super::event_loop::AppEvent;
use super::reducer::{ShellAction, ShellEffect};
use super::slash_command::SlashCommand;
use super::state::{AppState, OverlayState, RunPickerItem};
use super::transcript::ShellMessage;

#[derive(Clone)]
pub struct ShellController {
    client: TuiDaemonClient,
}

impl ShellController {
    pub fn new(client: TuiDaemonClient) -> Self {
        Self { client }
    }

    pub async fn ensure_daemon(&self) -> Result<()> {
        self.client.ensure_daemon().await
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
            ShellEffect::SubmitMessage { message } => {
                self.submit_message_effect(state, message, tx).await?;
                Ok(Vec::new())
            }
            ShellEffect::ExecuteSlashCommand(command) => self.slash_command_actions(state, command).await,
            ShellEffect::RejectSelectedApproval => self.reject_selected_approval_actions(state).await,
            ShellEffect::ClearScreen => Ok(Vec::new()),
        }
    }

    async fn refresh_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let sessions: Vec<ChatSessionSummary> = self.client.list_sessions().await.unwrap_or_default();
        let runs = if let Some(session_id) = state.current_session_id() {
            self.client
                .list_runs_for_session(session_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut actions = vec![ShellAction::StateRefreshed {
            sessions,
            runs,
        }];

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

    async fn overlay_selection_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        match state.overlay.clone() {
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
            Some(OverlayState::RunPicker { .. }) => {
                let Some(RunPickerItem::Run { run_id, .. }) = state.selected_run_picker_item() else {
                    return Ok(Vec::new());
                };
                let thread = self.client.get_execution_run_thread(&run_id).await?;
                let child_runs = self.client.list_child_runs(&run_id).await.unwrap_or_default();
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
            Some(OverlayState::ApprovalPicker { .. }) => self.approve_selected_approval_actions(state).await,
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

    async fn slash_command_actions(
        &self,
        state: &AppState,
        command: SlashCommand,
    ) -> Result<Vec<ShellAction>> {
        match command {
            SlashCommand::Help => Ok(vec![ShellAction::Ui(super::keymap::Action::OpenHelp)]),
            SlashCommand::TaskControl { action, task_id } => {
                let task = self.client.control_task(&task_id, action.as_str()).await?;
                Ok(vec![ShellAction::TaskControlCompleted {
                    task_id: task.id,
                    status: format!("{:?}", task.status),
                }])
            }
            SlashCommand::OpenRun { run_id } => {
                let thread = self.client.get_execution_run_thread(&run_id).await?;
                let child_runs = self.client.list_child_runs(&run_id).await.unwrap_or_default();
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
            SlashCommand::TeamState { team_run_id } => self.load_team_actions(&team_run_id, true).await,
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
                    bail!(output.error.unwrap_or_else(|| "manage_teams failed".to_string()));
                }
                let team_state = serde_json::from_value::<TeamState>(output.result["team"].clone())
                    .ok()
                    .ok_or_else(|| anyhow::anyhow!("start_team did not return team state"))?;
                let team_run_id = team_state.team_run_id.clone();
                let mut actions = vec![ShellAction::MessageAppended(ShellMessage::TeamNotice {
                    content: format!("Started team {team_run_id}"),
                })];
                actions.extend(self.load_team_actions(&team_run_id, true).await?);
                Ok(actions)
            }
            SlashCommand::Approve { approval_id } => self.approve_named_approval_actions(state, &approval_id).await,
            SlashCommand::Reject {
                approval_id,
                reason,
            } => self.reject_named_approval_actions(state, &approval_id, reason).await,
        }
    }

    async fn reject_selected_approval_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let approval_id = state
            .selected_approval()
            .map(|approval| approval.approval_id.clone())
            .ok_or_else(|| anyhow::anyhow!("No approval selected"))?;
        self.reject_named_approval_actions(state, &approval_id, None).await
    }

    async fn approve_selected_approval_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let approval_id = state
            .selected_approval()
            .map(|approval| approval.approval_id.clone())
            .ok_or_else(|| anyhow::anyhow!("No approval selected"))?;
        self.approve_named_approval_actions(state, &approval_id).await
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
            bail!(output.error.unwrap_or_else(|| "approval failed".to_string()));
        }

        let mut actions = self.load_team_actions(&team_run_id, false).await?;
        actions.push(ShellAction::MessageAppended(ShellMessage::TeamNotice {
            content: format!("Approval {approval_id} approved"),
        }));
        actions.push(ShellAction::StatusUpdated(format!("Approved {approval_id}")));
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
        actions.push(ShellAction::StatusUpdated(format!("Rejected {approval_id}")));
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
