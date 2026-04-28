use anyhow::{Result, bail};
use restflow_contracts::request::{ChildRunListQuery, WireModelRef};
use restflow_core::daemon::ChatSessionEvent;
use restflow_core::daemon::{
    DaemonConfig, IpcClient, IpcRequest, StreamFrame, is_daemon_available,
    start_daemon_with_config, stop_daemon,
};
use restflow_core::models::{
    ChatSession, ChatSessionSummary, ExecutionContainerKind, ExecutionContainerRef,
    ExecutionThread, ModelMetadataDTO, RunListQuery, RunSummary, Skill, Task,
};
use restflow_core::paths;
use restflow_core::storage::agent::{
    DEFAULT_ASSISTANT_NAME, LEGACY_DEFAULT_ASSISTANT_NAME, StoredAgent,
};
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

use super::event_loop::AppEvent;

#[derive(Clone)]
pub struct TuiDaemonClient {
    socket_path: PathBuf,
}

impl TuiDaemonClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            socket_path: paths::socket_path()?,
        })
    }

    pub async fn daemon_running(&self) -> bool {
        is_daemon_available(&self.socket_path).await
    }

    pub async fn start_daemon(&self) -> Result<()> {
        if self.daemon_running().await {
            return Ok(());
        }

        let report = restflow_core::daemon::recovery::recover().await?;
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
        let mut client = self.connect().await?;
        client.list_agents().await
    }

    pub async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        let mut client = self.connect().await?;
        client.get_agent(id.to_string()).await
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

        if let Some(agent) = agents
            .iter()
            .find(|agent| {
                agent
                    .name
                    .eq_ignore_ascii_case(LEGACY_DEFAULT_ASSISTANT_NAME)
            })
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
                let mut client = self.connect().await?;
                client.get_session(session_id.to_string()).await.map(Some)
            }
            None => Ok(None),
        }
    }

    pub async fn create_session_for_agent(
        &self,
        agent_id: &str,
        model: Option<&str>,
    ) -> Result<ChatSession> {
        let mut client = self.connect().await?;
        client
            .create_session(
                Some(agent_id.to_string()),
                model.map(ToOwned::to_owned),
                None,
                None,
            )
            .await
    }

    pub async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.connect().await?;
        client.list_sessions().await
    }

    pub async fn list_available_models(&self) -> Result<Vec<ModelMetadataDTO>> {
        let mut client = self.connect().await?;
        client.request_typed(IpcRequest::GetAvailableModels).await
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

    pub async fn list_skills(&self) -> Result<Vec<Skill>> {
        let mut client = self.connect().await?;
        client.list_skills().await
    }

    pub async fn get_skill(&self, skill_id: &str) -> Result<Option<Skill>> {
        let mut client = self.connect().await?;
        client.get_skill(skill_id.to_string()).await
    }

    pub async fn delete_skill(&self, skill_id: &str) -> Result<()> {
        let mut client = self.connect().await?;
        client.delete_skill(skill_id.to_string()).await
    }

    pub async fn get_session(&self, session_id: &str) -> Result<ChatSession> {
        let mut client = self.connect().await?;
        client.get_session(session_id.to_string()).await
    }

    pub async fn switch_session_model(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
    ) -> Result<ChatSession> {
        let mut client = self.connect().await?;
        client
            .request_typed(IpcRequest::SwitchSessionModel {
                session_id: session_id.to_string(),
                model_ref: WireModelRef {
                    provider: provider.to_string(),
                    model: model.to_string(),
                },
                reason: Some("tui model picker".to_string()),
            })
            .await
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool> {
        let mut client = self.connect().await?;
        client.delete_session(session_id.to_string()).await
    }

    pub async fn cancel_chat_stream(&self, stream_id: &str) -> Result<bool> {
        let mut client = self.connect().await?;
        client
            .cancel_chat_session_stream(stream_id.to_string())
            .await
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
            })
            .await
    }

    pub fn spawn_session_events(
        &self,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
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
            let mut ipc = match client.connect().await {
                Ok(ipc) => ipc,
                Err(error) => {
                    let _ = tx.send(AppEvent::Error(error.to_string()));
                    return;
                }
            };

            let result = ipc
                .subscribe_task_events(task_id.clone(), |event| {
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
        tokio::spawn(async move {
            let mut ipc = match client.connect().await {
                Ok(ipc) => ipc,
                Err(error) => {
                    let _ = tx.send(AppEvent::Error(error.to_string()));
                    return;
                }
            };
            let result = ipc
                .execute_chat_session_stream(
                    session_id.clone(),
                    Some(input),
                    stream_id,
                    |frame: StreamFrame| {
                        tx.send(AppEvent::StreamFrame(frame))
                            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                        Ok(())
                    },
                )
                .await;

            if let Err(error) = result {
                let _ = tx.send(AppEvent::Error(format!("Chat stream failed: {error}")));
            }
        })
    }
}
