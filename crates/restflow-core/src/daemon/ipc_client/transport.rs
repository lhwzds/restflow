#[cfg(unix)]
use super::*;

#[cfg(unix)]
impl IpcClient {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .context("Failed to connect to daemon. Is it running?")?;
        Ok(Self { stream })
    }

    pub(super) async fn send_request_frame(&mut self, req: &IpcRequest) -> Result<()> {
        let json = serde_json::to_vec(&req)?;
        self.stream
            .write_all(&(json.len() as u32).to_le_bytes())
            .await?;
        self.stream.write_all(&json).await?;
        Ok(())
    }

    pub(super) async fn read_raw_frame(&mut self) -> Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            anyhow::bail!("Response too large");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    pub async fn request(&mut self, req: IpcRequest) -> Result<IpcResponse> {
        self.send_request_frame(&req).await?;
        let buf = self.read_raw_frame().await?;
        Ok(serde_json::from_slice(&buf)?)
    }

    pub async fn ping(&mut self) -> bool {
        matches!(self.request(IpcRequest::Ping).await, Ok(IpcResponse::Pong))
    }

    pub async fn get_status(&mut self) -> Result<IpcDaemonStatus> {
        self.request_typed(IpcRequest::GetStatus).await
    }

    pub(super) async fn request_typed<T: DeserializeOwned>(
        &mut self,
        req: IpcRequest,
    ) -> Result<T> {
        match self.request(req).await? {
            IpcResponse::Success(value) => {
                serde_json::from_value(value).context("Failed to deserialize response")
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
            IpcResponse::Error {
                code,
                message,
                details,
            } => {
                bail!(Self::format_ipc_error(code, &message, details))
            }
        }
    }

    pub(super) async fn request_optional<T: DeserializeOwned>(
        &mut self,
        req: IpcRequest,
    ) -> Result<Option<T>> {
        match self.request(req).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error {
                code,
                message,
                details,
            } => {
                bail!(Self::format_ipc_error(code, &message, details))
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub(super) fn format_ipc_error(
        code: i32,
        message: &str,
        details: Option<serde_json::Value>,
    ) -> String {
        match details {
            Some(details) => serde_json::json!({
                "code": code,
                "message": message,
                "details": details
            })
            .to_string(),
            None => format!("IPC error {}: {}", code, message),
        }
    }
}

#[cfg(unix)]
pub async fn is_daemon_available(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    match IpcClient::connect(socket_path).await {
        Ok(mut client) => client.ping().await,
        Err(_) => false,
    }
}
