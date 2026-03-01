use anyhow::Result;
use restflow_core::daemon::{
    IPC_PROTOCOL_VERSION, IpcClient, IpcDaemonStatus, IpcRequest, is_daemon_available,
};
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

    pub async fn ensure_handshake(&mut self) -> Result<IpcDaemonStatus> {
        let client = self.ensure_connected().await?;
        let status = client.get_status().await?;
        Self::validate_handshake(&status)?;
        Ok(status)
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

    fn validate_handshake(status: &IpcDaemonStatus) -> Result<()> {
        if status.status != "running" {
            anyhow::bail!("Daemon handshake failed: status is '{}'", status.status);
        }
        if status.protocol_version != IPC_PROTOCOL_VERSION {
            anyhow::bail!(
                "Daemon handshake failed: protocol mismatch (daemon={}, expected={})",
                status.protocol_version,
                IPC_PROTOCOL_VERSION
            );
        }
        if status.daemon_version.trim().is_empty() {
            anyhow::bail!("Daemon handshake failed: daemon version is empty");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_status() -> IpcDaemonStatus {
        IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: IPC_PROTOCOL_VERSION.to_string(),
            daemon_version: "0.3.5".to_string(),
            pid: 1234,
            started_at_ms: 1_700_000_000_000,
            uptime_secs: 15,
        }
    }

    #[test]
    fn validate_handshake_accepts_matching_protocol() {
        let status = sample_status();
        assert!(DaemonManager::validate_handshake(&status).is_ok());
    }

    #[test]
    fn validate_handshake_rejects_protocol_mismatch() {
        let mut status = sample_status();
        status.protocol_version = "999".to_string();
        let err = DaemonManager::validate_handshake(&status).unwrap_err();
        assert!(err.to_string().contains("protocol mismatch"));
    }
}
