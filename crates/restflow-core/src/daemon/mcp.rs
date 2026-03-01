use crate::AppCore;
use crate::mcp::RestFlowMcpServer;
use anyhow::Result;
use bytes::Bytes;
use http::{
    Response, StatusCode,
    header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderValue},
};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tracing::{debug, error, info, warn};

type McpHttpBody = BoxBody<Bytes, Infallible>;
type McpHttpResponse = Response<McpHttpBody>;

const ERROR_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const RECOVERY_HEADER: &str = "x-restflow-mcp-recover";
const RECOVERY_REINITIALIZE: &str = "reinitialize";

pub async fn run_mcp_http_server(
    core: Arc<AppCore>,
    addr: SocketAddr,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let cancellation = CancellationToken::new();
    let config = build_streamable_http_server_config(cancellation.clone());

    let service = StreamableHttpService::new(
        {
            let core = core.clone();
            move || Ok(RestFlowMcpServer::new(core.clone()))
        },
        LocalSessionManager::default().into(),
        config,
    );
    let service = ServiceBuilder::new()
        .map_response(normalize_mcp_error_response)
        .service(service);
    let service = TowerToHyperService::new(service);

    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "MCP HTTP server listening");

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                cancellation.cancel();
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer_addr)) => {
                        let io = TokioIo::new(stream);
                        let service = service.clone();
                        tokio::spawn(async move {
                            if let Err(err) = Builder::new(TokioExecutor::default())
                                .serve_connection(io, service)
                                .await
                            {
                                let err_text = err.to_string();
                                if is_expected_connection_close(&err_text) {
                                    debug!(
                                        %peer_addr,
                                        error = %err_text,
                                        "MCP HTTP connection closed"
                                    );
                                } else {
                                    warn!(
                                        %peer_addr,
                                        error = %err_text,
                                        "MCP HTTP connection ended with error"
                                    );
                                }
                            }
                        });
                    }
                    Err(err) => {
                        error!(error = %err, "MCP HTTP accept error");
                    }
                }
            }
        }
    }

    Ok(())
}

fn build_streamable_http_server_config(
    cancellation_token: CancellationToken,
) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig {
        // Stateless mode avoids stale in-memory session IDs after daemon restarts.
        stateful_mode: false,
        cancellation_token,
        ..Default::default()
    }
}

fn classify_error_status(status: StatusCode) -> Option<(&'static str, &'static str, bool)> {
    match status {
        StatusCode::UNAUTHORIZED => Some((
            "session_expired",
            "MCP session is missing or expired. Reinitialize the session and retry.",
            true,
        )),
        StatusCode::NOT_ACCEPTABLE => Some((
            "invalid_accept_header",
            "The client must accept the required MCP media types.",
            false,
        )),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => Some((
            "invalid_content_type",
            "Content-Type must be application/json for MCP POST requests.",
            false,
        )),
        StatusCode::UNPROCESSABLE_ENTITY => Some((
            "invalid_request",
            "The request payload does not satisfy MCP protocol requirements.",
            false,
        )),
        StatusCode::METHOD_NOT_ALLOWED => Some((
            "method_not_allowed",
            "The HTTP method is not supported for this MCP endpoint.",
            false,
        )),
        _ => None,
    }
}

fn normalize_mcp_error_response(response: McpHttpResponse) -> McpHttpResponse {
    if response.headers().contains_key(CONTENT_TYPE) {
        return response;
    }

    let status = response.status();
    let Some((code, message, recoverable)) = classify_error_status(status) else {
        return response;
    };

    let mut builder = Response::builder().status(status);
    for (name, value) in response.headers() {
        if name == CONTENT_TYPE || name == CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(name, value.clone());
    }
    builder = builder.header(CONTENT_TYPE, HeaderValue::from_static(ERROR_CONTENT_TYPE));
    if recoverable {
        builder = builder.header(
            RECOVERY_HEADER,
            HeaderValue::from_static(RECOVERY_REINITIALIZE),
        );
    }

    let payload = json!({
        "error": {
            "code": code,
            "message": message,
            "recoverable": recoverable
        }
    });

    builder
        .body(Full::new(Bytes::from(payload.to_string())).boxed())
        .expect("valid MCP error response")
}

fn is_expected_connection_close(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    [
        "connection closed before message completed",
        "connection error",
        "connection reset by peer",
        "broken pipe",
        "operation canceled",
        "cancelled",
        "closed",
        "eof",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::{
        ERROR_CONTENT_TYPE, RECOVERY_HEADER, RECOVERY_REINITIALIZE,
        build_streamable_http_server_config, is_expected_connection_close,
        normalize_mcp_error_response,
    };
    use bytes::Bytes;
    use http::{
        Response, StatusCode,
        header::{CONTENT_TYPE, HeaderValue},
    };
    use http_body_util::{BodyExt, Full};
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn classify_expected_connection_shutdown_errors() {
        assert!(is_expected_connection_close(
            "connection closed before message completed"
        ));
        assert!(is_expected_connection_close("connection error"));
        assert!(is_expected_connection_close("broken pipe"));
    }

    #[test]
    fn classify_unexpected_connection_errors() {
        assert!(!is_expected_connection_close("tls handshake failed"));
        assert!(!is_expected_connection_close("http parse failure"));
    }

    #[test]
    fn mcp_http_server_config_uses_stateless_mode() {
        let config = build_streamable_http_server_config(CancellationToken::new());
        assert!(!config.stateful_mode);
    }

    #[tokio::test]
    async fn normalize_mcp_error_response_adds_json_error_payload() {
        let response = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Full::new(Bytes::from_static(b"Unauthorized: Session not found")).boxed())
            .unwrap();

        let normalized = normalize_mcp_error_response(response);

        assert_eq!(
            normalized.headers().get(CONTENT_TYPE).unwrap(),
            &HeaderValue::from_static(ERROR_CONTENT_TYPE)
        );
        assert_eq!(
            normalized.headers().get(RECOVERY_HEADER).unwrap(),
            &HeaderValue::from_static(RECOVERY_REINITIALIZE)
        );

        let body = normalized.into_body().collect().await.unwrap().to_bytes();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "session_expired");
        assert_eq!(payload["error"]["recoverable"], true);
    }

    #[tokio::test]
    async fn normalize_mcp_error_response_keeps_existing_content_type() {
        let response = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(CONTENT_TYPE, "text/plain")
            .body(Full::new(Bytes::from_static(b"already tagged")).boxed())
            .unwrap();

        let normalized = normalize_mcp_error_response(response);
        assert_eq!(
            normalized.headers().get(CONTENT_TYPE).unwrap(),
            "text/plain"
        );

        let body = normalized.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"already tagged");
    }
}
