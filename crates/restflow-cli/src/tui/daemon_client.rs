use anyhow::{Result, bail};
use restflow_contracts::request::ChildRunListQuery;
use restflow_core::daemon::ChatSessionEvent;
use restflow_core::daemon::{IpcClient, IpcRequest, StreamFrame, is_daemon_available};
use restflow_core::models::{
    ChatSession, ChatSessionSource, ChatSessionSummary,
    ExecutionContainerKind, ExecutionContainerRef, ExecutionThread, RunListQuery, RunSummary, Task,
};
use restflow_core::paths;
use restflow_core::storage::agent::{DEFAULT_ASSISTANT_NAME, LEGACY_DEFAULT_ASSISTANT_NAME, StoredAgent};
use restflow_contracts::ToolExecutionResult;
use std::path::PathBuf;
use tokio::sync::mpsc;

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

    pub async fn ensure_daemon(&self) -> Result<()> {
        if is_daemon_available(&self.socket_path).await {
            return Ok(());
        }
        bail!("RestFlow daemon is not running. Start it with 'restflow daemon start'.")
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

    pub async fn resolve_default_agent(&self, explicit: Option<&str>) -> Result<Option<StoredAgent>> {
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
            .find(|agent| agent.name.eq_ignore_ascii_case(LEGACY_DEFAULT_ASSISTANT_NAME))
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
        agent: &StoredAgent,
        session_override: Option<&str>,
    ) -> Result<Option<ChatSession>> {
        if let Some(session_id) = session_override {
            let mut client = self.connect().await?;
            return client.get_session(session_id.to_string()).await.map(Some);
        }

        let mut client = self.connect().await?;
        let sessions = client.list_full_sessions().await?;
        if let Some(existing) = sessions
            .into_iter()
            .filter(|session| session.agent_id == agent.id)
            .filter(|session| session.source_channel.is_none() || session.source_channel == Some(ChatSessionSource::Workspace))
            .max_by_key(|session| session.updated_at)
        {
            return Ok(Some(existing));
        }

        client
            .create_session(Some(agent.id.clone()), None, None, None)
            .await
            .map(Some)
    }

    pub async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.connect().await?;
        client.list_sessions().await
    }

    pub async fn get_session(&self, session_id: &str) -> Result<ChatSession> {
        let mut client = self.connect().await?;
        client.get_session(session_id.to_string()).await
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

    pub async fn execute_runtime_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
        let mut client = self.connect().await?;
        client.execute_tool(name.to_string(), input).await
    }

    pub fn spawn_session_events(&self, tx: mpsc::UnboundedSender<AppEvent>) -> tokio::task::JoinHandle<()> {
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
            let stream_id = uuid::Uuid::new_v4().to_string();
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
            } else {
                let _ = tx.send(AppEvent::RefreshCurrentSession);
            }
        })
    }
}
