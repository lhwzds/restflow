#[cfg(unix)]
use super::*;

#[cfg(unix)]
impl IpcClient {
    pub async fn execute_chat_session_stream<F>(
        &mut self,
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
        mut on_frame: F,
    ) -> Result<()>
    where
        F: FnMut(StreamFrame) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::ExecuteChatSessionStream {
            session_id,
            user_input,
            stream_id,
        })
        .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            let value: Value = serde_json::from_slice(&buf)
                .context("Failed to deserialize streaming IPC frame")?;
            // StreamFrame::Error and IpcResponse::Error share the same serde tag.
            // Treat only payloads with `details` as structured IPC errors.
            let has_structured_details = value
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "Error")
                && value
                    .get("data")
                    .and_then(|data| data.get("details"))
                    .is_some();
            if has_structured_details {
                let response: IpcResponse = serde_json::from_value(value.clone())
                    .context("Failed to decode structured IPC error while reading stream")?;
                if let IpcResponse::Error {
                    code,
                    message,
                    details,
                } = response
                {
                    bail!("{}", Self::format_ipc_error(code, &message, details));
                }
            }

            if let Ok(frame) = serde_json::from_value::<StreamFrame>(value.clone()) {
                let terminal =
                    matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error { .. });
                on_frame(frame)?;
                if terminal {
                    break;
                }
                continue;
            }

            let response: IpcResponse = serde_json::from_value(value)
                .context("Failed to deserialize streaming IPC frame")?;
            match response {
                IpcResponse::Error {
                    code,
                    message,
                    details,
                } => {
                    bail!("{}", Self::format_ipc_error(code, &message, details));
                }
                IpcResponse::Success(_) => {
                    bail!("Unexpected success response while reading stream")
                }
                IpcResponse::Pong => {
                    bail!("Unexpected Pong response while reading stream")
                }
            }
        }

        Ok(())
    }

    pub async fn cancel_chat_session_stream(&mut self, stream_id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct CancelResponse {
            canceled: bool,
        }
        let resp: CancelResponse = self
            .request_typed(IpcRequest::CancelChatSessionStream { stream_id })
            .await?;
        Ok(resp.canceled)
    }

    pub async fn steer_chat_session_stream(
        &mut self,
        session_id: String,
        instruction: String,
    ) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct SteerResponse {
            steered: bool,
        }
        let resp: SteerResponse = self
            .request_typed(IpcRequest::SteerChatSessionStream {
                session_id,
                instruction,
            })
            .await?;
        Ok(resp.steered)
    }
    pub async fn subscribe_background_agent_events<F>(
        &mut self,
        background_agent_id: String,
        mut on_event: F,
    ) -> Result<()>
    where
        F: FnMut(TaskStreamEvent) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::SubscribeBackgroundAgentEvents {
            background_agent_id,
        })
        .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            let value: Value = serde_json::from_slice(&buf)
                .context("Failed to deserialize background stream frame")?;
            let has_structured_details = value
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "Error")
                && value
                    .get("data")
                    .and_then(|data| data.get("details"))
                    .is_some();
            if has_structured_details {
                let response: IpcResponse = serde_json::from_value(value.clone()).context(
                    "Failed to decode structured IPC error while reading background stream",
                )?;
                if let IpcResponse::Error {
                    code,
                    message,
                    details,
                } = response
                {
                    bail!("{}", Self::format_ipc_error(code, &message, details));
                }
            }

            if let Ok(frame) = serde_json::from_value::<StreamFrame>(value.clone()) {
                match frame {
                    StreamFrame::Start { .. } => {}
                    StreamFrame::BackgroundAgentEvent { event } => {
                        on_event(event)?;
                    }
                    StreamFrame::Error { code, message } => {
                        bail!("Background event stream error {}: {}", code, message);
                    }
                    StreamFrame::Done { .. } => break Ok(()),
                    _ => {}
                }
                continue;
            }

            let response: IpcResponse = serde_json::from_value(value)
                .context("Failed to deserialize background stream frame")?;
            match response {
                IpcResponse::Error {
                    code,
                    message,
                    details,
                } => {
                    bail!("{}", Self::format_ipc_error(code, &message, details));
                }
                IpcResponse::Success(_) => {
                    bail!("Unexpected success response while reading background stream")
                }
                IpcResponse::Pong => {
                    bail!("Unexpected Pong response while reading background stream")
                }
            }
        }
    }

    pub async fn subscribe_session_events<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(ChatSessionEvent) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::SubscribeSessionEvents)
            .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            let value: Value = serde_json::from_slice(&buf)
                .context("Failed to deserialize session event stream frame")?;
            let has_structured_details = value
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "Error")
                && value
                    .get("data")
                    .and_then(|data| data.get("details"))
                    .is_some();
            if has_structured_details {
                let response: IpcResponse = serde_json::from_value(value.clone()).context(
                    "Failed to decode structured IPC error while reading session event stream",
                )?;
                if let IpcResponse::Error {
                    code,
                    message,
                    details,
                } = response
                {
                    bail!("{}", Self::format_ipc_error(code, &message, details));
                }
            }

            if let Ok(frame) = serde_json::from_value::<StreamFrame>(value.clone()) {
                match frame {
                    StreamFrame::Start { .. } => {}
                    StreamFrame::SessionEvent { event } => {
                        on_event(event)?;
                    }
                    StreamFrame::Error { code, message } => {
                        bail!("Session event stream error {}: {}", code, message);
                    }
                    StreamFrame::Done { .. } => break Ok(()),
                    _ => {}
                }
                continue;
            }

            let response: IpcResponse = serde_json::from_value(value)
                .context("Failed to deserialize session event stream frame")?;
            match response {
                IpcResponse::Error {
                    code,
                    message,
                    details,
                } => {
                    bail!("{}", Self::format_ipc_error(code, &message, details));
                }
                _ => {
                    bail!("Unexpected response while reading session event stream")
                }
            }
        }
    }
}
