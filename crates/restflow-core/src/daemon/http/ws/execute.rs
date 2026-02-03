use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use serde_json::json;

pub async fn execute_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let _ = socket.recv().await;
    let payload = json!({
        "error": "Streaming execution is not implemented in daemon HTTP API yet."
    });
    let _ = socket
        .send(Message::Text(payload.to_string()))
        .await;
    let _ = socket.close().await;
}
