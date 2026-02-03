use crate::daemon_client::DaemonClient;
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

pub async fn ws_proxy_handler(
    ws: WebSocketUpgrade,
    State(daemon): State<Arc<DaemonClient>>,
) -> impl IntoResponse {
    let ws_url = daemon.ws_url("/execute");

    ws.on_upgrade(move |socket| async move {
        if let Err(err) = proxy_websocket(socket, ws_url).await {
            tracing::warn!("WebSocket proxy error: {}", err);
        }
    })
}

async fn proxy_websocket(socket: WebSocket, ws_url: String) -> anyhow::Result<()> {
    let (mut daemon_ws, _) = tokio_tungstenite::connect_async(ws_url).await?;

    let (mut client_sender, mut client_receiver) = socket.split();
    let (mut daemon_sender, mut daemon_receiver) = daemon_ws.split();

    let client_to_daemon = async {
        while let Some(msg) = client_receiver.next().await {
            let msg = msg?;
            let outbound = match msg {
                Message::Text(text) => TungsteniteMessage::Text(text),
                Message::Binary(bytes) => TungsteniteMessage::Binary(bytes),
                Message::Ping(bytes) => TungsteniteMessage::Ping(bytes),
                Message::Pong(bytes) => TungsteniteMessage::Pong(bytes),
                Message::Close(frame) => {
                    let close =
                        frame.map(
                            |frame| tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                code: frame.code,
                                reason: frame.reason,
                            },
                        );
                    TungsteniteMessage::Close(close)
                }
            };

            daemon_sender.send(outbound).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    let daemon_to_client = async {
        while let Some(msg) = daemon_receiver.next().await {
            let msg = msg?;
            let outbound = match msg {
                TungsteniteMessage::Text(text) => Message::Text(text),
                TungsteniteMessage::Binary(bytes) => Message::Binary(bytes),
                TungsteniteMessage::Ping(bytes) => Message::Ping(bytes),
                TungsteniteMessage::Pong(bytes) => Message::Pong(bytes),
                TungsteniteMessage::Close(frame) => {
                    let close = frame.map(|frame| axum::extract::ws::CloseFrame {
                        code: frame.code,
                        reason: frame.reason,
                    });
                    Message::Close(close)
                }
                TungsteniteMessage::Frame(_) => continue,
            };

            client_sender.send(outbound).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        result = client_to_daemon => {
            if let Err(err) = result {
                tracing::debug!("Client to daemon closed: {}", err);
            }
        }
        result = daemon_to_client => {
            if let Err(err) = result {
                tracing::debug!("Daemon to client closed: {}", err);
            }
        }
    }

    Ok(())
}
