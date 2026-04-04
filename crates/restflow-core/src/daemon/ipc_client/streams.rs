#[cfg(unix)]
use super::*;
#[cfg(unix)]
use restflow_contracts::{CancelResponse, SteerResponse};

#[cfg(unix)]
fn read_stream_frame_or_ipc_error(
    buf: &[u8],
    deserialize_context: &str,
    unexpected_success: &str,
    unexpected_pong: &str,
) -> Result<StreamFrame> {
    if let Ok(frame) = serde_json::from_slice::<StreamFrame>(buf) {
        return Ok(frame);
    }

    let response: IpcResponse =
        serde_json::from_slice(buf).with_context(|| deserialize_context.to_string())?;
    match response {
        IpcResponse::Error(error) => bail!("{}", IpcClient::format_ipc_error(&error)),
        IpcResponse::Success(_) => bail!("{}", unexpected_success),
        IpcResponse::Pong => bail!("{}", unexpected_pong),
    }
}

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
            let frame = read_stream_frame_or_ipc_error(
                &buf,
                "Failed to deserialize streaming IPC frame",
                "Unexpected success response while reading stream",
                "Unexpected Pong response while reading stream",
            )?;
            let terminal = matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error(_));
            on_frame(frame)?;
            if terminal {
                break;
            }
        }

        Ok(())
    }

    pub async fn cancel_chat_session_stream(&mut self, stream_id: String) -> Result<bool> {
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
        let resp: SteerResponse = self
            .request_typed(IpcRequest::SteerChatSessionStream {
                session_id,
                instruction,
            })
            .await?;
        Ok(resp.steered)
    }
    pub async fn subscribe_task_events<F>(&mut self, task_id: String, mut on_event: F) -> Result<()>
    where
        F: FnMut(TaskStreamEvent) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::SubscribeTaskEvents { task_id })
            .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            match read_stream_frame_or_ipc_error(
                &buf,
                "Failed to deserialize task stream frame",
                "Unexpected success response while reading task stream",
                "Unexpected Pong response while reading task stream",
            )? {
                StreamFrame::Start { .. } => {}
                StreamFrame::Event {
                    event: IpcStreamEvent::BackgroundAgent(event),
                } => {
                    on_event(event)?;
                }
                StreamFrame::Error(error) => {
                    bail!(
                        "Task event stream error: {}",
                        Self::format_ipc_error(&error)
                    );
                }
                StreamFrame::Done { .. } => break Ok(()),
                _ => {}
            }
        }
    }

    pub async fn subscribe_background_agent_events<F>(
        &mut self,
        background_agent_id: String,
        on_event: F,
    ) -> Result<()>
    where
        F: FnMut(TaskStreamEvent) -> Result<()>,
    {
        self.subscribe_task_events(background_agent_id, on_event)
            .await
    }

    pub async fn subscribe_session_events<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(ChatSessionEvent) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::SubscribeSessionEvents)
            .await?;

        loop {
            let buf = self.read_raw_frame().await?;
            match read_stream_frame_or_ipc_error(
                &buf,
                "Failed to deserialize session event stream frame",
                "Unexpected success response while reading session event stream",
                "Unexpected Pong response while reading session event stream",
            )? {
                StreamFrame::Start { .. } => {}
                StreamFrame::Event {
                    event: IpcStreamEvent::Session(event),
                } => {
                    on_event(event)?;
                }
                StreamFrame::Error(error) => {
                    bail!(
                        "Session event stream error: {}",
                        Self::format_ipc_error(&error)
                    );
                }
                StreamFrame::Done { .. } => break Ok(()),
                _ => {}
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn read_stream_frame_or_ipc_error_accepts_done_frame() {
        let encoded = serde_json::to_vec(&StreamFrame::Done {
            total_tokens: Some(7),
        })
        .unwrap();

        let decoded = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap();

        assert!(matches!(
            decoded,
            StreamFrame::Done {
                total_tokens: Some(7)
            }
        ));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_accepts_error_frame() {
        let encoded = serde_json::to_vec(&StreamFrame::error(500, "boom")).unwrap();

        let decoded = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap();

        assert!(matches!(decoded, StreamFrame::Error(_)));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_surfaces_ipc_error() {
        let encoded = serde_json::to_vec(&IpcResponse::error(404, "missing session")).unwrap();

        let err = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap_err();

        assert!(err.to_string().contains("missing session"));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_rejects_unexpected_success() {
        let encoded =
            serde_json::to_vec(&IpcResponse::success(serde_json::json!({ "ok": true }))).unwrap();

        let err = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap_err();

        assert!(err.to_string().contains("unexpected success"));
    }

    #[test]
    fn read_stream_frame_or_ipc_error_rejects_unexpected_pong() {
        let encoded = serde_json::to_vec(&IpcResponse::Pong).unwrap();

        let err = read_stream_frame_or_ipc_error(
            &encoded,
            "decode failed",
            "unexpected success",
            "unexpected pong",
        )
        .unwrap_err();

        assert!(err.to_string().contains("unexpected pong"));
    }
}
