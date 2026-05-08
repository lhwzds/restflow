use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

use runtime::models::{
    ChatSession, ChatSessionSource, ChatSessionSummary, ModelId, ModelMetadataDTO, RunSummary,
    SkillSource,
};
use runtime::storage::agent::StoredAgent;

use super::daemon_client::TuiDaemonClient;
use super::event_loop::AppEvent;
use super::reducer::{ShellAction, ShellEffect};
use super::slash_command::{SLASH_COMMAND_SPECS, SlashCommand};
use super::state::{
    AppState, ModelPickerCategory, ModelPickerItem, OverlayState, PendingSessionState,
    ProviderPickerItem, SkillManagerSelection, SkillPickerItem, TaskPickerItem, WorkPickerItem,
    build_work_picker_items,
};

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

    pub async fn pending_session_for_agent(&self, agent: &StoredAgent) -> PendingSessionState {
        let mut pending = PendingSessionState::from_agent(agent);
        let available = self
            .client
            .list_available_models()
            .await
            .unwrap_or_default();
        let sessions = self.client.list_sessions().await.unwrap_or_default();
        if let Some(item) = select_default_model_item(
            &sessions,
            &available,
            Some((&pending.provider, &pending.model)),
        ) {
            pending.update_model(item.provider, item.model, item.name);
        }
        pending
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
            ShellEffect::SubmitMessage { message, stream_id } => {
                self.submit_message_effect(state, message, stream_id, tx)
                    .await?;
                Ok(Vec::new())
            }
            ShellEffect::SteerMessage {
                session_id,
                instruction,
            } => self.steer_message_actions(session_id, instruction).await,
            ShellEffect::CancelStream { stream_id } => self.cancel_stream_actions(stream_id).await,
            ShellEffect::ExecuteSlashCommand(command) => {
                self.slash_command_actions(state, command).await
            }
            ShellEffect::DeleteSession { session_id } => {
                self.delete_session_actions(session_id).await
            }
            ShellEffect::ListSkillsForMention => self.skill_mention_picker_actions().await,
            ShellEffect::ListSessionsInline => self.session_picker_actions().await,
            ShellEffect::ListRunsInline => self.list_runs_inline_actions(state).await,
            ShellEffect::ClearScreen => Ok(Vec::new()),
        }
    }

    async fn refresh_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let mut sessions: Vec<ChatSessionSummary> = if should_refresh_session_list(state) {
            self.client.list_sessions().await.unwrap_or_default()
        } else {
            state.sessions.clone()
        };
        if matches!(state.overlay, Some(OverlayState::SessionPicker { .. })) {
            let bound_session_ids = self
                .client
                .list_background_bound_session_ids()
                .await
                .unwrap_or_default();
            sessions = filter_resume_sessions(sessions, &bound_session_ids);
        }
        let runs = if let Some(session_id) = state.current_session_id() {
            self.client
                .list_runs_for_session(session_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let child_runs = if should_refresh_child_runs(state) {
            self.child_runs_for_runs(&runs).await
        } else {
            state.thread.child_runs.clone()
        };
        let tasks = self.task_items().await.unwrap_or_default();

        if refreshed_state_is_unchanged(state, &sessions, &runs, &child_runs, &tasks) {
            return Ok(Vec::new());
        }

        let actions = vec![ShellAction::StateRefreshed {
            sessions,
            runs,
            child_runs,
            tasks,
        }];

        Ok(actions)
    }

    async fn reload_current_session_actions(&self, state: &AppState) -> Result<Vec<ShellAction>> {
        let session_id = match preferred_reload_session_id(state, None) {
            Some(session_id) => session_id,
            None if state.is_streaming || state.active_turn.is_some() => {
                return Ok(vec![ShellAction::CurrentSessionReloaded {
                    session: None,
                    runs: state.thread.runs.clone(),
                    child_runs: state.thread.child_runs.clone(),
                }]);
            }
            None if state.active_turn_has_tool_call() => {
                return Err(anyhow::anyhow!("No active session available."));
            }
            None => match self.newest_session_id().await {
                Some(session_id) => session_id,
                None => return self.refresh_actions(state).await,
            },
        };

        let session = self.client.get_session(&session_id).await.ok();
        let (runs, child_runs) = self
            .session_runs_for_reload(state, session.as_ref().map(|_| session_id.as_str()))
            .await;
        if state.is_streaming || state.active_turn.is_some() {
            return Ok(vec![ShellAction::CurrentSessionReloaded {
                session: session.map(Box::new),
                runs,
                child_runs,
            }]);
        }

        let mut actions = vec![ShellAction::CurrentSessionReloaded {
            session: session.map(Box::new),
            runs,
            child_runs,
        }];
        actions.extend(self.refresh_actions(state).await?);
        Ok(actions)
    }

    async fn newest_session_id(&self) -> Option<String> {
        self.client
            .list_sessions()
            .await
            .unwrap_or_default()
            .into_iter()
            .max_by_key(|summary| summary.updated_at)
            .map(|summary| summary.id)
    }

    async fn session_runs_for_reload(
        &self,
        state: &AppState,
        session_id: Option<&str>,
    ) -> (Vec<RunSummary>, Vec<RunSummary>) {
        let Some(session_id) = session_id else {
            return (Vec::new(), Vec::new());
        };
        let Ok(runs) = self.client.list_runs_for_session(session_id).await else {
            return (state.thread.runs.clone(), state.thread.child_runs.clone());
        };
        let child_runs = if should_refresh_child_runs(state) {
            self.child_runs_for_runs(&runs).await
        } else {
            state.thread.child_runs.clone()
        };
        (runs, child_runs)
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
        let pending_session = if session.is_none() {
            if let Some(agent) = agent.as_ref() {
                Some(self.pending_session_for_agent(agent).await)
            } else {
                None
            }
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
            pending_session,
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
                if matches!(spec.command, "/new") {
                    return Ok(vec![ShellAction::SubmitText {
                        text: "/new".to_string(),
                    }]);
                }
                if matches!(spec.command, "/task") {
                    return self.task_picker_actions().await;
                }
                if matches!(spec.command, "/skill") {
                    return self.skill_picker_actions().await;
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
                let child_runs = self.child_runs_for_runs(&runs).await;
                Ok(vec![ShellAction::SessionOpened {
                    session: Box::new(session),
                    runs,
                    child_runs,
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
            Some(OverlayState::SkillManager { .. }) => {
                let Some(item) = state.selected_skill_manager_item() else {
                    return Ok(Vec::new());
                };
                match item {
                    SkillManagerSelection::Skill(skill) => {
                        self.skill_detail_actions(skill.id).await
                    }
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
                let Some(item) = state.selected_run_picker_item() else {
                    return Ok(Vec::new());
                };
                self.work_picker_selection_actions(item).await
            }
            Some(OverlayState::SkillMentionPicker { .. })
            | Some(OverlayState::SkillDetail)
            | Some(OverlayState::RunDetail)
            | Some(OverlayState::Help)
            | None => Ok(Vec::new()),
        }
    }

    async fn submit_message_effect(
        &self,
        state: &AppState,
        message: String,
        stream_id: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let session_id = match state.current_session_id() {
            Some(session_id) => session_id.to_string(),
            None => bail!("No active session available."),
        };
        self.client
            .spawn_chat_stream(session_id, message, stream_id, tx);
        Ok(())
    }

    async fn steer_message_actions(
        &self,
        session_id: String,
        instruction: String,
    ) -> Result<Vec<ShellAction>> {
        match self.client.steer_chat_stream(session_id, instruction).await {
            Ok(true) => Ok(vec![ShellAction::StatusUpdated(
                "Queued update for current response.".to_string(),
            )]),
            Ok(false) => Ok(vec![ShellAction::StatusUpdated(
                "No active response accepted the queued update.".to_string(),
            )]),
            Err(error) => Ok(vec![ShellAction::Error(format!(
                "Failed to queue update: {error}"
            ))]),
        }
    }

    async fn cancel_stream_actions(&self, stream_id: String) -> Result<Vec<ShellAction>> {
        match self.client.cancel_chat_stream(&stream_id).await {
            Ok(true) => Ok(vec![ShellAction::StatusUpdated(
                "Canceled current response.".to_string(),
            )]),
            Ok(false) => Ok(vec![ShellAction::StatusUpdated(
                "No active response to cancel.".to_string(),
            )]),
            Err(error) => Ok(vec![ShellAction::Error(format!(
                "Failed to cancel response: {error}"
            ))]),
        }
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
            child_runs: Vec::new(),
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
            SlashCommand::NewChat => Ok(vec![ShellAction::NewChatStarted {
                status: "Started new chat".to_string(),
            }]),
            SlashCommand::Quit => Ok(vec![ShellAction::Quit]),
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
            SlashCommand::Help => Ok(vec![ShellAction::OpenHelpOverlay]),
            SlashCommand::ListSessions => self.session_picker_actions().await,
            SlashCommand::ListSkills => self.skill_picker_actions().await,
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
            SlashCommand::SwitchModel { model } => self.switch_model_actions(state, model).await,
            SlashCommand::TaskControl { action, task_id } => {
                let task = self.client.control_task(&task_id, action.as_str()).await?;
                Ok(vec![ShellAction::TaskControlCompleted {
                    task_id: task.id,
                    status: format!("{:?}", task.status),
                }])
            }
            SlashCommand::OpenRun { run_id } => {
                self.open_run_or_latest_task_run_actions(&run_id).await
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
        let tasks = self.task_items().await?;
        let status = if tasks.is_empty() {
            "No tasks available.".to_string()
        } else {
            "Select a task".to_string()
        };
        Ok(vec![ShellAction::TaskPickerLoaded { tasks, status }])
    }

    async fn skill_picker_actions(&self) -> Result<Vec<ShellAction>> {
        let skills = self.sorted_skill_items().await?;
        let status = if skills.is_empty() {
            "No skills installed.".to_string()
        } else {
            "View skills".to_string()
        };
        Ok(vec![ShellAction::SkillPickerLoaded { skills, status }])
    }

    async fn skill_mention_picker_actions(&self) -> Result<Vec<ShellAction>> {
        let skills = self.sorted_skill_items().await?;
        let status = if skills.is_empty() {
            "No skills installed.".to_string()
        } else {
            "Select a skill mention".to_string()
        };
        Ok(vec![ShellAction::SkillMentionPickerLoaded {
            skills,
            status,
        }])
    }

    async fn sorted_skill_items(&self) -> Result<Vec<SkillPickerItem>> {
        let mut skills = self
            .client
            .list_skills()
            .await?
            .into_iter()
            .map(SkillPickerItem::from)
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| {
            skill_source_order(left.source)
                .cmp(&skill_source_order(right.source))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(skills)
    }

    async fn skill_detail_actions(&self, skill_id: String) -> Result<Vec<ShellAction>> {
        match self.client.get_skill(&skill_id).await? {
            Some(skill) => Ok(vec![ShellAction::SkillDetailLoaded {
                status: format!("Showing skill {}", skill.id),
                skill: Box::new(skill),
            }]),
            None => Ok(vec![ShellAction::StatusUpdated(format!(
                "Skill not found: {skill_id}"
            ))]),
        }
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
        match self
            .client
            .switch_session_model(session_id, &item.provider, &item.model)
            .await
        {
            Ok(session) => Ok(vec![ShellAction::ModelSwitched {
                session: Box::new(session),
                status: format!("Switched model to {}", item.model),
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
        let tasks = self
            .task_items()
            .await
            .unwrap_or_else(|_| state.tasks.clone());
        let runs = if let Some(session_id) = state.current_session_id() {
            self.client
                .list_runs_for_session(session_id)
                .await
                .unwrap_or_else(|_| state.thread.runs.clone())
        } else {
            state.thread.runs.clone()
        };
        let child_runs = self.child_runs_for_runs(&runs).await;
        let items = build_work_picker_items(&tasks, &runs, &child_runs);
        let status = if items.is_empty() {
            "No active work, runs, or background tasks.".to_string()
        } else {
            "Work picker opened.".to_string()
        };

        Ok(vec![ShellAction::RunPickerLoaded {
            tasks,
            runs,
            child_runs,
            status,
        }])
    }

    async fn task_items(&self) -> Result<Vec<TaskPickerItem>> {
        let mut items = Vec::new();
        for task in self.client.list_tasks().await? {
            let latest_run_id = self
                .client
                .list_runs_for_task(&task.id)
                .await
                .ok()
                .and_then(|runs| latest_run_id(&runs));
            items.push(task_item_from_task(task, latest_run_id));
        }
        Ok(items)
    }

    async fn child_runs_for_runs(
        &self,
        runs: &[runtime::models::RunSummary],
    ) -> Vec<runtime::models::RunSummary> {
        let mut child_runs = Vec::new();
        for run in runs {
            let Some(run_id) = run.run_id.as_deref() else {
                continue;
            };
            child_runs.extend(
                self.client
                    .list_child_runs(run_id)
                    .await
                    .unwrap_or_default(),
            );
        }
        child_runs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        child_runs
    }

    async fn work_picker_selection_actions(
        &self,
        item: WorkPickerItem,
    ) -> Result<Vec<ShellAction>> {
        match item {
            WorkPickerItem::BackgroundTask {
                task_id,
                latest_run_id,
                ..
            } => {
                if let Some(run_id) = latest_run_id {
                    return self.open_run_id_actions(&run_id).await;
                }
                Ok(vec![ShellAction::OpenTaskActionPicker { task_id }])
            }
            WorkPickerItem::Run { run_id, .. } => self.open_run_id_actions(&run_id).await,
        }
    }

    async fn open_run_or_latest_task_run_actions(
        &self,
        identifier: &str,
    ) -> Result<Vec<ShellAction>> {
        let run_error = match self.open_run_id_actions(identifier).await {
            Ok(actions) => return Ok(actions),
            Err(error) => error,
        };

        if let Ok(runs) = self.client.list_runs_for_task(identifier).await
            && let Some(run_id) = latest_run_id(&runs)
        {
            return self.open_run_id_actions(&run_id).await;
        }

        Err(run_error)
    }

    async fn open_run_id_actions(&self, run_id: &str) -> Result<Vec<ShellAction>> {
        let thread = self.client.get_execution_run_thread(run_id).await?;
        let child_runs = self
            .client
            .list_child_runs(run_id)
            .await
            .unwrap_or_default();
        let session = if let Some(session_id) = thread.focus.session_id.as_deref() {
            self.client.get_session(session_id).await.ok()
        } else {
            None
        };
        Ok(vec![ShellAction::RunOpened {
            session: session.map(Box::new),
            run_id: run_id.to_string(),
            thread: Box::new(thread),
            child_runs,
            status: format!("Opened run {run_id}"),
        }])
    }
}

fn refreshed_state_is_unchanged(
    state: &AppState,
    sessions: &[ChatSessionSummary],
    runs: &[runtime::models::RunSummary],
    child_runs: &[runtime::models::RunSummary],
    tasks: &[TaskPickerItem],
) -> bool {
    if state.sessions != sessions {
        return false;
    }
    if state.tasks != tasks {
        return false;
    }
    if state.current_session_id().is_some() {
        state.thread.runs == runs && state.thread.child_runs == child_runs
    } else {
        runs.is_empty()
            && child_runs.is_empty()
            && state.thread.runs.is_empty()
            && state.thread.child_runs.is_empty()
    }
}

fn should_refresh_session_list(state: &AppState) -> bool {
    matches!(state.overlay, Some(OverlayState::SessionPicker { .. }))
}

fn should_refresh_child_runs(state: &AppState) -> bool {
    (!state.is_streaming && state.active_turn.is_none()) || state.activity.has_subagent_activity()
}

fn latest_run_id(runs: &[RunSummary]) -> Option<String> {
    runs.iter()
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        })
        .and_then(|run| run.run_id.clone())
}

fn task_item_from_task(
    task: runtime::models::Task,
    latest_run_id: Option<String>,
) -> TaskPickerItem {
    TaskPickerItem {
        task_id: task.id,
        name: task.name,
        status: format!("{:?}", task.status),
        next_run_at: task.next_run_at,
        latest_run_id,
    }
}

fn filter_resume_sessions(
    sessions: Vec<ChatSessionSummary>,
    bound_session_ids: &std::collections::HashSet<String>,
) -> Vec<ChatSessionSummary> {
    sessions
        .into_iter()
        .filter(|session| {
            session.message_count > 0
                && !bound_session_ids.contains(&session.id)
                && session.source_channel != Some(ChatSessionSource::Background)
                && !session.name.trim_start().starts_with("Background:")
        })
        .collect()
}

fn preferred_reload_session_id(
    state: &AppState,
    newest_session_id: Option<String>,
) -> Option<String> {
    state
        .active_refresh_session_id()
        .map(ToOwned::to_owned)
        .or_else(|| state.current_session_id().map(ToOwned::to_owned))
        .or(newest_session_id)
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

fn category_for_usage(
    key: &str,
    usage: Option<&ModelUsage>,
    recent_keys: &HashSet<String>,
) -> ModelPickerCategory {
    if recent_keys.contains(key) {
        ModelPickerCategory::Recent
    } else if usage.is_some_and(|usage| usage.count > 0) {
        ModelPickerCategory::Frequent
    } else {
        ModelPickerCategory::Available
    }
}

fn picker_catalog_metadata_by_key() -> HashMap<String, ModelMetadataDTO> {
    ModelId::all_with_metadata()
        .into_iter()
        .filter(|metadata| !metadata.model.is_opencode_cli() && !metadata.model.is_gemini_cli())
        .map(|metadata| {
            (
                model_key(
                    metadata.provider.as_canonical_str(),
                    metadata.model.as_serialized_str(),
                ),
                metadata,
            )
        })
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
    let mut items_by_provider = HashMap::<String, ProviderPickerItem>::new();

    for (provider, usage) in &usage {
        let category = category_for_usage(provider, Some(usage), &recent_keys);
        items_by_provider.insert(
            provider.clone(),
            ProviderPickerItem {
                label: provider.clone(),
                is_current: current_provider.as_deref() == Some(provider.as_str()),
                provider: provider.clone(),
                category,
                usage_count: usage.count,
                last_used_at: usage.last_used_at,
            },
        );
    }

    for metadata in available {
        let provider = metadata.provider.as_canonical_str().to_string();
        let usage = usage.get(&provider).cloned().unwrap_or_default();
        let category = category_for_usage(&provider, Some(&usage), &recent_keys);
        items_by_provider
            .entry(provider.clone())
            .and_modify(|item| {
                item.is_current = current_provider.as_deref() == Some(provider.as_str());
            })
            .or_insert_with(|| ProviderPickerItem {
                label: provider.clone(),
                is_current: current_provider.as_deref() == Some(provider.as_str()),
                provider,
                category,
                usage_count: usage.count,
                last_used_at: usage.last_used_at,
            });
    }

    if let Some(provider) = current_provider
        && !provider.trim().is_empty()
    {
        items_by_provider
            .entry(provider.clone())
            .or_insert_with(|| ProviderPickerItem {
                label: provider.clone(),
                is_current: true,
                provider,
                category: ModelPickerCategory::Recent,
                usage_count: 0,
                last_used_at: None,
            });
    }

    let mut items = items_by_provider.into_values().collect::<Vec<_>>();
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
    let catalog = picker_catalog_metadata_by_key();

    let current_key = current_model
        .map(|(provider, model)| model_key(provider, model))
        .filter(|key| key != ":");

    let mut items_by_key = HashMap::<String, ModelPickerItem>::new();
    let mut available_keys = HashSet::<String>::new();

    for metadata in available
        .iter()
        .filter(|metadata| metadata.provider.as_canonical_str() == provider_filter)
    {
        let provider = metadata.provider.as_canonical_str().to_string();
        let model = metadata.model.as_serialized_str().to_string();
        let key = model_key(&provider, &model);
        available_keys.insert(key.clone());
        let usage = usage.get(&key).cloned().unwrap_or_default();
        let category = category_for_usage(&key, Some(&usage), &recent_keys);
        items_by_key
            .entry(key.clone())
            .and_modify(|item| {
                item.name = metadata.name.clone();
                item.is_current = current_key.as_deref() == Some(key.as_str());
            })
            .or_insert_with(|| ModelPickerItem {
                provider,
                model,
                name: metadata.name.clone(),
                category,
                usage_count: usage.count,
                last_used_at: usage.last_used_at,
                is_current: current_key.as_deref() == Some(key.as_str()),
            });
    }

    if let Some(key) = current_key.as_ref()
        && available_keys.contains(key)
        && let Some(metadata) = catalog.get(key)
        && metadata.provider.as_canonical_str() == provider_filter
    {
        items_by_key
            .entry(key.clone())
            .or_insert_with(|| ModelPickerItem {
                provider: metadata.provider.as_canonical_str().to_string(),
                model: metadata.model.as_serialized_str().to_string(),
                name: metadata.name.clone(),
                category: ModelPickerCategory::Recent,
                usage_count: 0,
                last_used_at: None,
                is_current: true,
            });
    }

    let mut items = items_by_key.into_values().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        model_category_order(left.category)
            .cmp(&model_category_order(right.category))
            .then_with(|| match left.category {
                ModelPickerCategory::Recent => right.last_used_at.cmp(&left.last_used_at),
                ModelPickerCategory::Frequent => right.usage_count.cmp(&left.usage_count),
                ModelPickerCategory::Available => std::cmp::Ordering::Equal,
            })
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| {
                model_picker_sort_rank(&left.model).cmp(&model_picker_sort_rank(&right.model))
            })
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.model.cmp(&right.model))
    });
    items
}

fn model_picker_sort_rank(model: &str) -> usize {
    let gpt_5_4_codex = ModelId::Gpt5_4Codex.as_serialized_str();
    let gpt_5_4_mini_codex = ModelId::Gpt5_4MiniCodex.as_serialized_str();
    let codex_cli = ModelId::CodexCli.as_serialized_str();
    let gpt_5_codex = ModelId::Gpt5Codex.as_serialized_str();
    let gpt_5_1_codex = ModelId::Gpt5_1Codex.as_serialized_str();
    let gpt_5_2_codex = ModelId::Gpt5_2Codex.as_serialized_str();

    match ModelId::normalize_model_id(model) {
        Some(model) if model == gpt_5_4_codex => 0,
        Some(model) if model == gpt_5_4_mini_codex => 1,
        Some(model) if model == codex_cli => 2,
        Some(model) if model == gpt_5_codex || model == gpt_5_1_codex || model == gpt_5_2_codex => {
            20
        }
        _ => 10,
    }
}

fn select_default_model_item(
    sessions: &[ChatSessionSummary],
    available: &[ModelMetadataDTO],
    current_model: Option<(&str, &str)>,
) -> Option<ModelPickerItem> {
    let providers = build_provider_picker_items(sessions, available, current_model);
    providers.into_iter().find_map(|provider| {
        build_model_picker_items_for_provider(
            sessions,
            available,
            current_model,
            &provider.provider,
        )
        .into_iter()
        .next()
    })
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

fn skill_source_order(source: SkillSource) -> u8 {
    match source {
        SkillSource::System => 0,
        SkillSource::User => 1,
        SkillSource::External => 2,
    }
}

fn delete_session_error_message(session_id: &str, error: String) -> String {
    if error.contains("bound to task") {
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

#[cfg(test)]
mod tests {
    use super::{
        build_model_picker_items_for_provider, build_provider_picker_items,
        delete_session_error_message, filter_resume_sessions, preferred_reload_session_id,
        refreshed_state_is_unchanged, select_default_model_item, should_refresh_child_runs,
        should_refresh_session_list, start_daemon_error_actions,
    };
    use crate::reducer::ShellAction;
    use crate::state::{AppState, ModelPickerCategory, OverlayState, TaskPickerItem};
    use runtime::models::{
        ChatSession, ChatSessionSource, ChatSessionSummary, ModelId, ModelMetadataDTO,
    };
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
    fn unchanged_refresh_state_can_skip_render_action() {
        let mut state = AppState::empty();
        state.sessions = vec![session_summary_with_messages("session-1", "Chat", 2)];
        state.tasks = vec![TaskPickerItem {
            task_id: "task-1".to_string(),
            name: "Daily digest".to_string(),
            status: "Active".to_string(),
            next_run_at: Some(10),
            latest_run_id: None,
        }];

        assert!(refreshed_state_is_unchanged(
            &state,
            &state.sessions,
            &[],
            &[],
            &state.tasks,
        ));
    }

    #[test]
    fn changed_refresh_state_requires_render_action() {
        let mut state = AppState::empty();
        state.sessions = vec![session_summary_with_messages("session-1", "Chat", 2)];
        let refreshed = vec![session_summary_with_messages("session-1", "Chat", 3)];

        assert!(!refreshed_state_is_unchanged(
            &state,
            &refreshed,
            &[],
            &[],
            &state.tasks,
        ));
    }

    #[test]
    fn filter_resume_sessions_removes_background_bound_sessions() {
        let sessions = vec![
            session_summary_with_messages("session-1", "Regular", 1),
            session_summary_with_messages("session-2", "Background", 1),
            session_summary_with_messages("session-3", "Empty", 0),
            session_summary_with_messages("session-4", "Background: Reviewer", 1),
        ];
        let mut source_background = session_summary_with_messages("session-5", "Reviewer", 1);
        source_background.source_channel = Some(ChatSessionSource::Background);
        let mut sessions = sessions;
        sessions.push(source_background);
        let bound = HashSet::from(["session-2".to_string()]);

        let visible = filter_resume_sessions(sessions, &bound);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "session-1");
    }

    #[test]
    fn preferred_reload_session_keeps_active_anchor_over_newest_session() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session);
        state.push_local_user_message("run a tool".to_string());
        state.apply_stream_frame(types::StreamFrame::ToolCall {
            id: "call-1".to_string(),
            name: "edit".to_string(),
            arguments: serde_json::json!({"file_path":"check.txt"}),
        });

        assert_eq!(
            preferred_reload_session_id(&state, Some("newer-session".to_string())),
            Some(session_id)
        );
    }

    #[test]
    fn preferred_reload_session_uses_current_session_before_listing_newest() {
        let mut state = AppState::empty();
        let session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let session_id = session.id.clone();
        state.set_current_session(session);

        assert_eq!(
            preferred_reload_session_id(&state, Some("newer-session".to_string())),
            Some(session_id)
        );
    }

    #[test]
    fn active_turn_refresh_keeps_hot_path_off_global_session_and_child_run_lists() {
        let mut state = AppState::empty();
        state.set_current_session(ChatSession::new("agent-1".to_string(), "model".to_string()));
        state.push_local_user_message("hello".to_string());
        state.begin_stream("turn-1".to_string());

        assert!(!should_refresh_session_list(&state));
        assert!(!should_refresh_child_runs(&state));
    }

    #[test]
    fn idle_refresh_keeps_global_session_list_off_hot_path() {
        let state = AppState::empty();

        assert!(!should_refresh_session_list(&state));
        assert!(should_refresh_child_runs(&state));
    }

    #[test]
    fn session_picker_refresh_updates_global_session_list() {
        let mut state = AppState::empty();
        state.overlay = Some(OverlayState::SessionPicker { selected: 0 });

        assert!(should_refresh_session_list(&state));
    }

    #[test]
    fn delete_session_error_message_summarizes_background_bound_conflict() {
        let message = delete_session_error_message(
            "session-1",
            "IPC error 409: Session session-1 is bound to task task-1".to_string(),
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
    fn provider_picker_includes_used_providers_without_current_api_key() {
        let sessions = vec![
            session_summary_with_model("session-1", "Codex", "codex", "gpt-5.4", 100),
            session_summary_with_model(
                "session-2",
                "Zai",
                "zai-coding-plan",
                "zai-coding-plan-glm-5-1",
                90,
            ),
        ];
        let available = vec![model_metadata(ModelId::Gpt5, "GPT-5")];
        let items = build_provider_picker_items(&sessions, &available, Some(("codex", "gpt-5.4")));
        let providers = items
            .iter()
            .map(|item| item.provider.as_str())
            .collect::<Vec<_>>();

        assert!(providers.contains(&"codex"));
        assert!(providers.contains(&"zai-coding-plan"));
        assert!(providers.contains(&"openai"));
        assert!(
            items
                .iter()
                .any(|item| item.provider == "codex" && item.is_current)
        );
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

    #[test]
    fn model_picker_includes_used_models_without_current_api_key() {
        let sessions = vec![session_summary_with_model(
            "session-1",
            "Codex",
            "codex",
            "gpt-5.4",
            100,
        )];
        let available = vec![model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4")];
        let items = build_model_picker_items_for_provider(&sessions, &available, None, "codex");

        assert_eq!(items[0].provider, "codex");
        assert_eq!(items[0].model, "gpt-5.4");
        assert_eq!(items[0].name, "GPT-5.4");
        assert_eq!(items[0].category, ModelPickerCategory::Recent);
    }

    #[test]
    fn model_picker_excludes_used_models_without_available_metadata() {
        let sessions = vec![session_summary_with_model(
            "session-1",
            "Old OpenAI",
            "openai",
            "gpt-5-2",
            100,
        )];
        let available = vec![model_metadata(ModelId::Gpt5_4, "GPT-5.4")];
        let items = build_model_picker_items_for_provider(&sessions, &available, None, "openai");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].model, "gpt-5-4");
    }

    #[test]
    fn default_model_selection_skips_unavailable_current_model() {
        let sessions = vec![session_summary_with_model(
            "session-1",
            "DeepSeek",
            "deepseek",
            "deepseek-chat",
            100,
        )];
        let available = vec![model_metadata(ModelId::DeepseekChat, "DeepSeek Chat")];

        let item = select_default_model_item(&sessions, &available, Some(("openai", "gpt-5.4")))
            .expect("available fallback model");

        assert_eq!(item.provider, "deepseek");
        assert_eq!(item.model, "deepseek-chat");
    }

    #[test]
    fn default_model_selection_prefers_supported_codex_default() {
        let available = vec![
            model_metadata(ModelId::Gpt5Codex, "GPT-5 Codex"),
            model_metadata(ModelId::Gpt5_1Codex, "GPT-5.1 Codex"),
            model_metadata(ModelId::Gpt5_4MiniCodex, "GPT-5.4 Mini"),
            model_metadata(ModelId::Gpt5_4Codex, "GPT-5.4"),
            model_metadata(ModelId::CodexCli, "GPT-5.3 Codex"),
        ];

        let item = select_default_model_item(&[], &available, Some(("codex", "gpt-5-codex")))
            .expect("codex default model");

        assert_eq!(item.provider, "codex");
        assert_eq!(item.model, "gpt-5.4");
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
