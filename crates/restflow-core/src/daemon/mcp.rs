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
use tracing::{error, info};

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
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        let service = service.clone();
                        tokio::spawn(async move {
                            if let Err(err) = Builder::new(TokioExecutor::default())
                                .serve_connection(io, service)
                                .await
                            {
                                error!(error = %err, "MCP HTTP connection error");
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
