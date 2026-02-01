use anyhow::{Result, anyhow};
use futures::StreamExt;
use restflow_core::channel::{Channel, InboundMessage, OutboundMessage, TelegramChannel};
use restflow_core::process::ProcessRegistry;
use restflow_core::storage::Storage;
use restflow_tauri_lib::{AgentExecutor, RealAgentExecutor};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::config::CliConfig;

const TELEGRAM_BOT_TOKEN_SECRET: &str = "TELEGRAM_BOT_TOKEN";

pub struct TelegramAgent {
    channel: TelegramChannel,
    agent_id: String,
    executor: Arc<RealAgentExecutor>,
}

pub struct TelegramAgentHandle {
    shutdown_tx: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

impl TelegramAgentHandle {
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.join_handle.await;
    }
}

impl TelegramAgent {
    pub fn from_storage(storage: Arc<Storage>) -> Result<Option<Self>> {
        let token = match storage.secrets.get_secret(TELEGRAM_BOT_TOKEN_SECRET)? {
            Some(token) if !token.trim().is_empty() => token,
            _ => return Ok(None),
        };

        let agent_id = resolve_agent_id(&storage)?;
        let process_registry = Arc::new(ProcessRegistry::new());
        let executor = Arc::new(RealAgentExecutor::new(storage, process_registry));
        let channel = TelegramChannel::with_token(token);

        Ok(Some(Self {
            channel,
            agent_id,
            executor,
        }))
    }

    pub fn start(self) -> TelegramAgentHandle {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let mut stream = self.channel.start_receiving();
        let agent_id = self.agent_id.clone();
        let executor = self.executor.clone();
        let channel = self.channel;

        let join_handle = tokio::spawn(async move {
            let Some(mut stream) = stream.take() else {
                warn!("Telegram agent disabled: channel not configured");
                return;
            };

            info!("Telegram agent started (agent_id={})", agent_id);

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Telegram agent stopping");
                        break;
                    }
                    message = stream.next() => {
                        let Some(message) = message else {
                            warn!("Telegram message stream ended");
                            break;
                        };

                        if let Err(err) = handle_message(&channel, &executor, &agent_id, message).await {
                            error!("Telegram agent error: {}", err);
                        }
                    }
                }
            }
        });

        TelegramAgentHandle {
            shutdown_tx,
            join_handle,
        }
    }
}

fn resolve_agent_id(storage: &Storage) -> Result<String> {
    let config = CliConfig::load();
    if let Some(agent_ref) = config
        .default
        .agent
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if let Some(agent) = storage.agents.get_agent(agent_ref.to_string())? {
            return Ok(agent.id);
        }

        let agents = storage.agents.list_agents()?;
        if let Some(agent) = agents.iter().find(|agent| agent.name == agent_ref) {
            return Ok(agent.id.clone());
        }

        warn!(
            "Configured default agent '{}' not found; falling back",
            agent_ref
        );
    }

    let agents = storage.agents.list_agents()?;
    let agent = agents
        .first()
        .ok_or_else(|| anyhow!("No agents available. Create one first."))?;

    Ok(agent.id.clone())
}

async fn handle_message(
    channel: &TelegramChannel,
    executor: &RealAgentExecutor,
    agent_id: &str,
    message: InboundMessage,
) -> Result<()> {
    let response = match executor.execute(agent_id, Some(&message.content)).await {
        Ok(result) => result.output,
        Err(err) => {
            let reply = OutboundMessage::error(&message.conversation_id, err.to_string())
                .with_reply_to(&message.id);
            channel.send(reply).await?;
            return Ok(());
        }
    };

    let reply = OutboundMessage::new(&message.conversation_id, response).with_reply_to(&message.id);

    channel.send(reply).await?;
    Ok(())
}
