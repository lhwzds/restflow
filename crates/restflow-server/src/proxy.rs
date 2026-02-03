use crate::daemon_client::DaemonClient;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::IntoResponse,
    routing::any,
};
use std::sync::Arc;

pub fn proxy_router(daemon: Arc<DaemonClient>) -> Router {
    Router::new()
        .route("/*path", any(proxy_handler))
        .with_state(daemon)
}

async fn proxy_handler(
    State(daemon): State<Arc<DaemonClient>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let path = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());
    let headers = filter_headers(headers);

    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => Some(bytes.to_vec()),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", err),
            )
                .into_response();
        }
    };

    match daemon.forward(method, path, headers, body_bytes).await {
        Ok(response) => build_response(response).await,
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            format!("Failed to reach daemon: {}", err),
        )
            .into_response(),
    }
}

async fn build_response(response: reqwest::Response) -> axum::response::Response {
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to read daemon response: {}", err),
            )
                .into_response();
        }
    };

    let mut builder = axum::response::Response::builder().status(status);
    if let Some(header_map) = builder.headers_mut() {
        for (name, value) in headers.iter() {
            if is_hop_by_hop(name.as_str()) {
                continue;
            }
            header_map.append(name, value.clone());
        }
    }

    builder
        .body(Body::from(bytes))
        .unwrap_or_else(|_| (StatusCode::BAD_GATEWAY, "Failed to build response").into_response())
}

fn filter_headers(headers: HeaderMap) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name.as_str()) || name == axum::http::header::HOST {
            continue;
        }
        filtered.append(name, value.clone());
    }
    filtered
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}
