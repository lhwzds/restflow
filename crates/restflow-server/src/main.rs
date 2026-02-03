mod static_assets;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod api;
mod auth;

use api::{
    agent_tasks::*, agents::*, chat_sessions::*, config::*, memory::memory_routes, mcp::mcp_router,
    models::*, python::*, secrets::*, security::*, skills::*, state::AppState, tools::*,
};
use auth::{auth_middleware, ApiKeyManager};
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

    // Create AppState with security checker and auth manager
    let shared_state = AppState::new(core.clone())
        .await
        .expect("Failed to initialize AppState");
    let api_key_manager = ApiKeyManager::from_env();

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
        // Agent tasks
        .route("/api/agent-tasks", get(list_agent_tasks).post(create_agent_task))
        .route(
            "/api/agent-tasks/{id}",
            get(get_agent_task)
                .put(update_agent_task)
                .delete(delete_agent_task),
        )
        .route("/api/agent-tasks/{id}/pause", post(pause_agent_task))
        .route("/api/agent-tasks/{id}/resume", post(resume_agent_task))
        .route("/api/agent-tasks/{id}/run", post(run_agent_task))
        .route("/api/agent-tasks/{id}/events", get(get_agent_task_events))
        .route("/api/agent-tasks/{id}/stream", get(stream_agent_task_events))
        // Chat sessions
        .route("/api/chat-sessions", get(list_chat_sessions).post(create_chat_session))
        .route(
            "/api/chat-sessions/{id}",
            get(get_chat_session)
                .patch(update_chat_session)
                .delete(delete_chat_session),
        )
        .route(
            "/api/chat-sessions/{id}/messages",
            post(add_chat_message),
        )
        .route(
            "/api/chat-sessions/{id}/summary",
            get(get_chat_session_summary),
        )
        // Security
        .route("/api/security/approvals", get(list_pending_approvals))
        .route(
            "/api/security/approvals/{id}/approve",
            post(approve_security_approval),
        )
        .route(
            "/api/security/approvals/{id}/reject",
            post(reject_security_approval),
        )
        .route(
            "/api/security/allowlist",
            get(get_security_allowlist).put(update_security_allowlist),
        )
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
        // Memory routes (search, chunks, stats, export, import)
        .nest("/api/memory", memory_routes())
        // MCP routes
        .nest("/mcp", mcp_router(core.clone()))
        .fallback(static_assets::static_handler)
        .layer(cors)
        .layer(axum::middleware::from_fn(auth_middleware))
        .layer(axum::Extension(api_key_manager))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    tracing::info!("RestFlow running on http://localhost:3000");

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}
