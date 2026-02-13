use crate::AppCore;
use crate::mcp::RestFlowMcpServer;
use anyhow::Result;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub async fn run_mcp_http_server(
    core: Arc<AppCore>,
    addr: SocketAddr,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let cancellation = CancellationToken::new();
    let config = StreamableHttpServerConfig {
        cancellation_token: cancellation.clone(),
        ..Default::default()
    };

    let service = TowerToHyperService::new(StreamableHttpService::new(
        {
            let core = core.clone();
            move || Ok(RestFlowMcpServer::new(core.clone()))
        },
        LocalSessionManager::default().into(),
        config,
    ));

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
    use super::is_expected_connection_close;

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
}
