mod static_assets;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod api;

use api::{agents::*, config::*, models::*, python::*, secrets::*, skills::*, tools::*};
use axum::{
    Router,
    http::{Method, header},
    routing::{get, post, put},
};
use restflow_core::{AppCore, paths};
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
async fn main() {
    // Initialize tracing logger
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,restflow_server=debug".into()),
        )
        .with_target(false)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    tracing::info!("Starting RestFlow backend server");

    // Use AppCore for unified initialization
    let db_path =
        paths::ensure_database_path_string().expect("Failed to determine RestFlow database path");
    let core = Arc::new(
        AppCore::new(&db_path)
            .await
            .expect("Failed to initialize app core"),
    );

    // Configure CORS
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

    // AppState is now just an alias for Arc<AppCore>
    let shared_state = core.clone();

    let app = Router::new()
        .route("/health", get(health))
        // System configuration
        .route("/api/config", get(get_config).put(update_config))
        // AI models metadata
        .route("/api/models", get(list_models))
        // AI tools
        .route("/api/tools", get(list_tools))
        // Python integration
        .route("/api/python/execute", post(execute_script))
        .route("/api/python/scripts", get(list_scripts))
        .route("/api/python/templates", get(list_templates))
        .route("/api/python/templates/{template_id}", get(get_template))
        // Agent management
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
        .route("/api/agents/{id}/execute", post(execute_agent))
        .route("/api/agents/execute-inline", post(execute_agent_inline))
        // Secret management
        .route("/api/secrets", get(list_secrets).post(create_secret))
        .route(
            "/api/secrets/{key}",
            put(update_secret).delete(delete_secret),
        )
        // Skills management
        .route("/api/skills", get(list_skills).post(create_skill))
        .route("/api/skills/import", post(import_skill))
        .route(
            "/api/skills/{id}",
            get(get_skill).put(update_skill).delete(delete_skill),
        )
        .route("/api/skills/{id}/export", get(export_skill))
        .fallback(static_assets::static_handler)
        .layer(cors)
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    tracing::info!("RestFlow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}
