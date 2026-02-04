use crate::AppCore;
use axum::{routing::get, Extension, Router};
use std::sync::Arc;

use super::{api, middleware, ws, HttpConfig};

pub fn build_router(core: Arc<AppCore>, config: &HttpConfig) -> Router {
    let cors = middleware::cors::build_cors_layer(config);

    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/api/execute", get(ws::execute_handler))
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

    app
}

async fn health_check() -> &'static str {
    "OK"
}
