pub mod agents;
pub mod auth;
pub mod config;
pub mod memory;
pub mod secrets;
pub mod sessions;
pub mod skills;
pub mod background_agents;

use axum::Router;

/// Build the main API router with all resource routes
pub fn router() -> Router {
    Router::new()
        .nest("/agents", agents::router())
        .nest("/skills", skills::router())
        .nest("/background-agents", background_agents::router())
        .nest("/memory", memory::router())
        .nest("/sessions", sessions::router())
        .nest("/secrets", secrets::router())
        .nest("/auth", auth::router())
        .nest("/config", config::router())
}
