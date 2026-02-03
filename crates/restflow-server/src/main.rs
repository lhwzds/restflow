mod config;
mod daemon_client;
mod middleware;
mod proxy;
mod static_assets;
mod ws_proxy;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use crate::config::ServerConfig;
use crate::daemon_client::DaemonClient;
use crate::middleware::{ApiKeyManager, RateLimiter, auth_middleware, rate_limit_middleware};
use crate::proxy::proxy_router;
use crate::ws_proxy::ws_proxy_handler;
use axum::{
    Router,
    http::{Method, header},
    routing::get,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[derive(serde::Serialize)]
struct Health {
    status: String,
}

async fn health() -> axum::Json<Health> {
    axum::Json(Health {
        status: "restflow is working!".to_string(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,restflow_server=debug".into()),
        )
        .with_target(false)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    tracing::info!("Starting RestFlow gateway server");

    let config = ServerConfig::load()?;
    let daemon = Arc::new(DaemonClient::new(&config.daemon_url));
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute);
    let api_key_manager = ApiKeyManager::from_env();

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = Router::new()
        .route("/health", get(health))
        .nest("/api", proxy_router(daemon.clone()))
        .route("/execute", get(ws_proxy_handler))
        .fallback(static_assets::static_handler)
        .layer(cors)
        .layer(axum::middleware::from_fn(rate_limit_middleware))
        .layer(axum::middleware::from_fn(auth_middleware))
        .layer(axum::Extension(rate_limiter))
        .layer(axum::Extension(api_key_manager))
        .layer(axum::Extension(daemon));

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|err| anyhow::anyhow!("Failed to bind to {}: {}", addr, err))?;

    tracing::info!("RestFlow gateway listening on http://{}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|err| anyhow::anyhow!("Failed to start server: {}", err))?;

    Ok(())
}
