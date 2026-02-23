use anyhow::Result;
use restflow_core::daemon::{IpcClient, IpcRequest, is_daemon_available};
use restflow_core::paths;
use std::path::PathBuf;
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

        Ok(self
            .client
            .as_mut()
            .expect("IPC client should be available"))
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

        let cli_bin = Self::find_cli_binary()?;
        let child = Command::new(cli_bin)
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

    /// Find the `restflow` CLI binary.
    ///
    /// Search order:
    /// 1. Same directory as the current executable (dev: target/debug/)
    /// 2. Well-known install locations (macOS .app doesn't inherit shell PATH)
    /// 3. `restflow` on PATH
    fn find_cli_binary() -> Result<PathBuf> {
        // Check sibling path next to current exe
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            let sibling = dir.join("restflow");
            if sibling.exists() {
                return Ok(sibling);
            }
        }

        // Check well-known install locations
        // macOS .app bundles get a minimal PATH from launchd,
        // so ~/.local/bin and ~/.cargo/bin are not included.
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for rel in &[".local/bin/restflow", ".cargo/bin/restflow"] {
                let p = home.join(rel);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
        for p in &["/usr/local/bin/restflow", "/opt/homebrew/bin/restflow"] {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Ok(p);
            }
        }

        // Fall back to PATH lookup
        if let Some(path) = Self::find_in_path("restflow") {
            return Ok(path);
        }

        anyhow::bail!(
            "Could not find `restflow` CLI binary. \
             Please install it or ensure it is on your PATH."
        )
    }

    /// Search for a binary name in the system PATH.
    fn find_in_path(name: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|p| p.is_file())
        })
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
