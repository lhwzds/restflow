use crate::AppCore;
use axum::{Extension, Router, routing::get};
use std::{path::Path, sync::Arc};
use tower_http::services::{ServeDir, ServeFile};

use super::{HttpConfig, api, middleware};

pub fn build_router(core: Arc<AppCore>, config: &HttpConfig) -> Router {
    let cors = middleware::cors::build_cors_layer(config);

    let mut app = Router::new()
        .route("/health", get(health_check))
        .nest("/api", api::router())
        .layer(cors)
        .layer(Extension(core));

    if config.auth_enabled {
        let api_key = config.api_key.clone();
        app = app.layer(axum::middleware::from_fn(move |req, next| {
            let api_key = api_key.clone();
            async move { middleware::auth::require_api_key(req, next, api_key).await }
        }));
    }

    let dist_dir = Path::new("web/dist");
    if dist_dir.exists() {
        let index = dist_dir.join("index.html");
        let static_service = ServeDir::new(dist_dir).not_found_service(ServeFile::new(index));
        app = app.fallback_service(static_service);
    }

    app
}

async fn health_check() -> &'static str {
    "OK"
}
