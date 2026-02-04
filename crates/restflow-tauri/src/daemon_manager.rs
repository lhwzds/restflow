use anyhow::Result;
use restflow_core::daemon::{IpcClient, IpcRequest, is_daemon_available};
use restflow_core::paths;
use std::process::{Child, Command, Stdio};
use tokio::time::{Duration, sleep};

pub struct DaemonManager {
    child_process: Option<Child>,
    client: Option<IpcClient>,
}

impl Default for DaemonManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonManager {
    pub fn new() -> Self {
        Self {
            child_process: None,
            client: None,
        }
    }

    pub async fn ensure_connected(&mut self) -> Result<&mut IpcClient> {
        // Check if we need to reconnect
        let needs_connect = match self.client.as_mut() {
            Some(client) => !client.ping().await,
            None => true,
        };

        if needs_connect {
            self.connect().await?;
        }

        Ok(self.client.as_mut().expect("IPC client should be available"))
    }

    async fn connect(&mut self) -> Result<()> {
        let socket_path = paths::socket_path()?;
        if !is_daemon_available(&socket_path).await {
            self.start_daemon().await?;
        }

        let client = IpcClient::connect(&socket_path).await?;
        self.client = Some(client);
        Ok(())
    }

    async fn start_daemon(&mut self) -> Result<()> {
        if self.child_process.is_some() {
            return Ok(());
        }

        let exe = std::env::current_exe()?;
        let child = Command::new(exe)
            .args(["daemon", "start", "--foreground"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        self.child_process = Some(child);
        self.wait_for_ready().await?;
        Ok(())
    }

    async fn wait_for_ready(&self) -> Result<()> {
        let socket_path = paths::socket_path()?;
        for _ in 0..400 {
            if is_daemon_available(&socket_path).await {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        anyhow::bail!("Daemon failed to start within timeout");
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(mut client) = self.client.take() {
            let _ = client.request(IpcRequest::Shutdown).await;
        }

        if let Some(mut child) = self.child_process.take() {
            let _ = child.wait();
        }

        Ok(())
    }
}
