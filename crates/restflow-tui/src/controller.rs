use anyhow::{Result, bail};
use serde_json::json;
use tokio::sync::mpsc;

use restflow_core::models::{ChatSession, ChatSessionSummary};
use restflow_core::storage::agent::StoredAgent;
use restflow_traits::{TeamAssignment, TeamMessage, TeamState};

use super::daemon_client::TuiDaemonClient;
use super::event_loop::AppEvent;
use super::reducer::{ShellAction, ShellEffect};
use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand};
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
            ShellEffect::ListRunsInline => self.list_runs_inline_actions(state).await,
            ShellEffect::ListApprovalsInline => Ok(self.list_approvals_inline_actions(state)),
            ShellEffect::ShowTeamInline => Ok(self.show_team_inline_actions(state)),
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
                let command = command_display(spec.command, spec.args);
                if matches!(spec.command, "/task") {
                    return Ok(vec![ShellAction::CommandPicked {
                        text: "/task ".to_string(),
                    }]);
                }
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
        let Some(agent_id) = state.default_agent_id.as_deref() else {
            bail!("No default agent configured. Create one from the standard CLI.");
        };
        let session = self.client.create_session_for_agent(agent_id).await?;
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
            SlashCommand::ListRuns => self.list_runs_inline_actions(state).await,
            SlashCommand::ListApprovals => Ok(self.list_approvals_inline_actions(state)),
            SlashCommand::ShowTeam => Ok(self.show_team_inline_actions(state)),
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

    fn show_team_inline_actions(&self, state: &AppState) -> Vec<ShellAction> {
        let Some(team) = state.current_team_state.as_ref() else {
            return vec![ShellAction::MessageAppended(ShellMessage::InfoNotice {
                content: "No team context for the current session.".to_string(),
            })];
        };

        let mut lines = vec![
            format!("Team {}", team.team_run_id),
            format!(
                "Leader: {} · Status: {:?}",
                team.leader_member_id, team.status
            ),
            format!(
                "Members: {} · Pending messages: {} · Pending assignments: {}",
                team.members.len(),
                team.pending_message_count,
                team.pending_assignment_count
            ),
        ];
        if !state.current_team_assignments.is_empty() {
            lines.push("Assignments".to_string());
            for assignment in &state.current_team_assignments {
                lines.push(format!(
                    "- {} -> {} · {:?}",
                    assignment.assignment_id, assignment.assignee_member_id, assignment.status
                ));
            }
        }
        if !state.current_team_approvals.is_empty() {
            lines.push(format!(
                "Pending approvals: {}",
                state.current_team_approvals.len()
            ));
        }

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
/team\n\
/task"
}

#[cfg(test)]
mod tests {
    use super::{delete_session_error_message, filter_resume_sessions, start_daemon_error_actions};
    use crate::reducer::ShellAction;
    use restflow_core::models::ChatSessionSummary;
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
}
