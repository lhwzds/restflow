use anyhow::{Result, bail};
use runtime::AppCore;
use runtime::daemon::{
    DaemonConfig, IpcClient, is_daemon_available, start_daemon_with_config, stop_daemon,
};
use runtime::models::{
    ChatSession, ChatSessionSummary, ExecutionContainerKind, ExecutionContainerRef,
    ExecutionThread, ModelId, ModelMetadataDTO, Provider, RunListQuery, RunSummary, Skill, Task,
    TaskSpec,
};
use runtime::paths;
use runtime::services::{session::SessionService, skills as skills_service};
use runtime::storage::agent::{DEFAULT_ASSISTANT_NAME, StoredAgent};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use types::request::{ChildRunListQuery, WireModelRef};
use types::{ChatSessionEvent, IpcRequest, StreamFrame};

use super::event_loop::AppEvent;

#[derive(Clone)]
pub struct TuiDaemonClient {
    socket_path: PathBuf,
    core: Arc<AppCore>,
}

impl TuiDaemonClient {
    pub async fn new() -> Result<Self> {
        let db_path = paths::ensure_database_path_string()?;
        let core = Arc::new(AppCore::new(&db_path).await?);
        Ok(Self {
            socket_path: paths::socket_path()?,
            core,
        })
    }

    pub async fn daemon_running(&self) -> bool {
        is_daemon_available(&self.socket_path).await
    }

    pub async fn start_daemon(&self) -> Result<()> {
        if self.daemon_running().await {
            return Ok(());
        }

        let report = runtime::daemon::recovery::recover().await?;
        let _ = report;
        tokio::task::spawn_blocking(|| start_daemon_with_config(DaemonConfig::default())).await??;

        for _ in 0..100 {
            if self.daemon_running().await {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }

        bail!("RestFlow daemon did not become ready in time.")
    }

    pub async fn stop_daemon(&self) -> Result<bool> {
        tokio::task::spawn_blocking(stop_daemon).await?
    }

    async fn connect(&self) -> Result<IpcClient> {
        IpcClient::connect(&self.socket_path).await
    }

    pub async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        self.core.storage.agents.list_agents()
    }

    pub async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        self.core
            .storage
            .agents
            .get_agent(id.to_string())?
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {id}"))
    }

    pub async fn resolve_default_agent(
        &self,
        explicit: Option<&str>,
    ) -> Result<Option<StoredAgent>> {
        if let Some(id) = explicit {
            return self.get_agent(id).await.map(Some);
        }

        let agents = self.list_agents().await?;
        if agents.is_empty() {
            return Ok(None);
        }

        if let Some(agent) = agents
            .iter()
            .find(|agent| agent.name.eq_ignore_ascii_case(DEFAULT_ASSISTANT_NAME))
            .cloned()
        {
            return Ok(Some(agent));
        }

        if agents.len() == 1 {
            return Ok(agents.into_iter().next());
        }

        bail!(
            "Default agent is ambiguous. Configure '{}' or pass --agent.",
            DEFAULT_ASSISTANT_NAME
        )
    }

    pub async fn resolve_or_create_session(
        &self,
        _agent: &StoredAgent,
        session_override: Option<&str>,
    ) -> Result<Option<ChatSession>> {
        match session_override {
            Some(session_id) => {
                SessionService::from_storage(&self.core.storage).get_session_view(session_id)
            }
            None => Ok(None),
        }
    }

    pub async fn create_session_for_agent(
        &self,
        agent_id: &str,
        model: Option<&str>,
    ) -> Result<ChatSession> {
        let agent_id = self
            .core
            .storage
            .agents
            .resolve_existing_agent_id(agent_id)?;
        let model = match model {
            Some(model) => normalize_model_input(model)?,
            None => self
                .core
                .storage
                .agents
                .get_agent(agent_id.clone())?
                .and_then(|agent| agent.agent.resolved_model_ref())
                .map(|model_ref| model_ref.model.as_serialized_str().to_string())
                .unwrap_or_else(|| ModelId::Gpt5_4.as_serialized_str().to_string()),
        };
        SessionService::from_storage(&self.core.storage)
            .create_workspace_session(agent_id, model, None, None, None)
    }

    pub async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        SessionService::from_storage(&self.core.storage).list_session_summaries(None, None, false)
    }

    pub async fn list_available_models(&self) -> Result<Vec<ModelMetadataDTO>> {
        Ok(available_model_catalog(&self.core))
    }

    pub async fn list_background_bound_session_ids(&self) -> Result<HashSet<String>> {
        let mut client = self.connect().await?;
        let tasks: Vec<Task> = client
            .request_typed(IpcRequest::ListTasks { status: None })
            .await?;
        Ok(tasks
            .into_iter()
            .filter_map(|task| {
                let session_id = task.chat_session_id.trim();
                (!session_id.is_empty()).then(|| session_id.to_string())
            })
            .collect())
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>> {
        let mut client = self.connect().await?;
        client
            .request_typed(IpcRequest::ListTasks { status: None })
            .await
    }

    pub async fn create_task(&self, spec: TaskSpec) -> Result<Task> {
        let mut client = self.connect().await?;
        client.create_task(spec).await
    }

    pub async fn list_skills(&self) -> Result<Vec<Skill>> {
        skills_service::list_skills(&self.core).await
    }

    pub async fn get_skill(&self, skill_id: &str) -> Result<Option<Skill>> {
        skills_service::get_skill(&self.core, skill_id).await
    }

    pub async fn get_session(&self, session_id: &str) -> Result<ChatSession> {
        SessionService::from_storage(&self.core.storage)
            .get_session_view(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))
    }

    pub async fn switch_session_model(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
    ) -> Result<ChatSession> {
        let _model_ref = WireModelRef {
            provider: provider.to_string(),
            model: model.to_string(),
        };
        let model = normalize_model_input(model)?;
        let session_service = SessionService::from_storage(&self.core.storage);
        let mut session = session_service
            .get_session_view(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        session.set_model_identity_from_raw(&model);
        session_service.save_session_metadata(&session)?;
        Ok(session)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool> {
        SessionService::from_storage(&self.core.storage).delete_session(session_id)
    }

    pub async fn cancel_chat_stream(&self, stream_id: &str) -> Result<bool> {
        Ok(runtime::daemon::cancel_foreground_chat_stream(&self.core, stream_id).await)
    }

    pub async fn list_runs_for_session(&self, session_id: &str) -> Result<Vec<RunSummary>> {
        let mut client = self.connect().await?;
        client
            .list_runs(RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Workspace,
                    id: session_id.to_string(),
                },
            })
            .await
    }

    pub async fn list_runs_for_task(&self, task_id: &str) -> Result<Vec<RunSummary>> {
        let mut client = self.connect().await?;
        client
            .list_runs(RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Task,
                    id: task_id.to_string(),
                },
            })
            .await
    }

    pub async fn get_execution_run_thread(&self, run_id: &str) -> Result<ExecutionThread> {
        let mut client = self.connect().await?;
        client
            .request_typed(IpcRequest::GetExecutionRunThread {
                run_id: run_id.to_string(),
            })
            .await
    }

    pub async fn list_child_runs(&self, parent_run_id: &str) -> Result<Vec<RunSummary>> {
        let mut client = self.connect().await?;
        client
            .request_typed(IpcRequest::ListChildRuns {
                query: ChildRunListQuery {
                    parent_run_id: parent_run_id.to_string(),
                },
            })
            .await
    }

    pub async fn control_task(&self, task_id: &str, action: &str) -> Result<Task> {
        let mut client = self.connect().await?;
        client
            .request_typed(IpcRequest::ControlTask {
                id: task_id.to_string(),
                action: action.to_string(),
                approval_id: None,
            })
            .await
    }

    pub fn spawn_session_events(
        &self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
            if !client.daemon_running().await {
                return;
            }
            let mut ipc = match client.connect().await {
                Ok(ipc) => ipc,
                Err(error) => {
                    let _ = tx.send(AppEvent::Error(error.to_string()));
                    return;
                }
            };

            let result = ipc
                .subscribe_session_events(|event: ChatSessionEvent| {
                    tx.send(AppEvent::SessionEvent(event))
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    Ok(())
                })
                .await;

            if let Err(error) = result {
                let _ = tx.send(AppEvent::Error(format!("Session stream stopped: {error}")));
            }
        })
    }

    pub fn spawn_task_events(
        &self,
        task_id: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
            if !client.daemon_running().await {
                return;
            }
            let mut ipc = match client.connect().await {
                Ok(ipc) => ipc,
                Err(error) => {
                    let _ = tx.send(AppEvent::Error(error.to_string()));
                    return;
                }
            };

            let result = ipc
                .subscribe_task_events(task_id.clone(), None, None, |event| {
                    tx.send(AppEvent::TaskEvent(event))
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    Ok(())
                })
                .await;

            if let Err(error) = result {
                let _ = tx.send(AppEvent::Error(format!(
                    "Task stream for {task_id} stopped: {error}"
                )));
            }
        })
    }

    pub fn spawn_chat_stream(
        &self,
        session_id: String,
        input: String,
        stream_id: String,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        let workspace_root = std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned());
        tokio::spawn(async move {
            let stream = runtime::daemon::open_foreground_chat_session_stream(
                client.core.clone(),
                session_id.clone(),
                Some(input),
                stream_id,
                workspace_root,
            )
            .await;
            let mut rx = match stream {
                Ok(rx) => rx,
                Err(error) => {
                    let _ = tx.send(AppEvent::Error(format!("Chat stream failed: {error}")));
                    return;
                }
            };
            let mut saw_terminal_frame = false;
            while let Some(frame) = rx.recv().await {
                saw_terminal_frame =
                    matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error(_));
                if tx.send(AppEvent::StreamFrame(frame)).is_err() {
                    break;
                }
            }

            if !saw_terminal_frame {
                let _ = tx.send(AppEvent::Error(
                    "Chat stream ended before a terminal frame.".to_string(),
                ));
            }
        })
    }

    pub async fn steer_chat_stream(&self, session_id: String, instruction: String) -> Result<bool> {
        Ok(
            runtime::daemon::steer_foreground_chat_stream(&self.core, &session_id, &instruction)
                .await,
        )
    }
}

fn normalize_model_input(model: &str) -> Result<String> {
    ModelId::normalize_model_id(model)
        .ok_or_else(|| anyhow::anyhow!("Unsupported model identifier: {}", model))
}

fn is_catalog_model(model: ModelId) -> bool {
    !model.is_opencode_cli() && !model.is_gemini_cli() && !is_legacy_openai_model(model)
}

fn is_legacy_openai_model(model: ModelId) -> bool {
    matches!(
        model,
        ModelId::Gpt5
            | ModelId::Gpt5Mini
            | ModelId::Gpt5Nano
            | ModelId::Gpt5Pro
            | ModelId::Gpt5_1
            | ModelId::Gpt5_2
    )
}

fn provider_has_secret(core: &AppCore, provider: Provider) -> bool {
    provider.api_key_env_candidates().any(|key| {
        core.storage
            .secrets
            .get_non_empty(key)
            .ok()
            .flatten()
            .is_some()
    })
}

fn available_model_catalog(core: &AppCore) -> Vec<ModelMetadataDTO> {
    let mut providers = Vec::new();
    for provider in Provider::all().iter().copied() {
        if provider == Provider::Codex || provider_has_secret(core, provider) {
            providers.push(provider);
        }
    }
    providers.sort_by_key(|provider| format!("{provider:?}"));

    let mut models = ModelId::all_with_metadata()
        .into_iter()
        .filter(|metadata| is_catalog_model(metadata.model))
        .filter(|metadata| providers.contains(&metadata.provider))
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        format!("{:?}", left.provider)
            .cmp(&format!("{:?}", right.provider))
            .then_with(|| model_sort_rank(left.model).cmp(&model_sort_rank(right.model)))
            .then_with(|| left.name.cmp(&right.name))
    });
    models
}

fn model_sort_rank(model: ModelId) -> usize {
    if model == ModelId::Gpt5_4Codex {
        return 0;
    }
    if model == ModelId::Gpt5_4MiniCodex {
        return 1;
    }
    if model == ModelId::CodexCli {
        return 2;
    }
    if model == ModelId::Gpt5Codex || model == ModelId::Gpt5_1Codex || model == ModelId::Gpt5_2Codex
    {
        return 20;
    }
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_catalog_sort_prefers_supported_default() {
        let mut models = [
            ModelId::Gpt5Codex,
            ModelId::Gpt5_1Codex,
            ModelId::Gpt5_4MiniCodex,
            ModelId::Gpt5_4Codex,
            ModelId::CodexCli,
        ];

        models.sort_by_key(|model| model_sort_rank(*model));

        assert_eq!(models[0], ModelId::Gpt5_4Codex);
        assert_eq!(models[1], ModelId::Gpt5_4MiniCodex);
        assert_eq!(models[2], ModelId::CodexCli);
    }
}
