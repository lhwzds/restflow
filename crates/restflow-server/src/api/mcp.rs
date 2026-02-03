use crate::auth::ApiKeyManager;
use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use restflow_core::{AppCore, mcp::RestFlowMcpServer};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::Deserialize;
use serde_json::json;
use std::{env, sync::Arc};

pub fn mcp_router(core: Arc<AppCore>) -> Router {
    let service = StreamableHttpService::new(
        {
            let core = core.clone();
            move || Ok(RestFlowMcpServer::new(core.clone()))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    Router::new()
        .nest_service("/", service)
        .layer(axum::middleware::from_fn(optional_api_key_auth))
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Claims {
    sub: Option<String>,
    exp: Option<usize>,
}

async fn optional_api_key_auth(req: Request, next: Next) -> Response {
    let has_api_keys = env::var("RESTFLOW_API_KEYS")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_jwt_secret = env::var("RESTFLOW_API_JWT_SECRET")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    if !has_api_keys && !has_jwt_secret {
        return next.run(req).await;
    }

    let token = match extract_token(req.headers()) {
        Some(token) => token,
        None => return unauthorized(),
    };

    if let Some(manager) = req.extensions().get::<Arc<ApiKeyManager>>()
        && manager.validate_key(&token).is_some()
    {
        return next.run(req).await;
    }

    if let Ok(secret) = env::var("RESTFLOW_API_JWT_SECRET") {
        let validation = Validation::new(Algorithm::HS256);
        let key = DecodingKey::from_secret(secret.as_bytes());
        if decode::<Claims>(&token, &key, &validation).is_ok() {
            return next.run(req).await;
        }
    }

    unauthorized()
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-api-key") {
        return value.to_str().ok().map(|token| token.trim().to_string());
    }

    extract_bearer(headers.get(axum::http::header::AUTHORIZATION))
}

fn extract_bearer(header: Option<&HeaderValue>) -> Option<String> {
    let value = header?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(|token| token.trim().to_string())
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "Unauthorized"})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Extension, body::Body, http::Request as HttpRequest};
    use tower::ServiceExt;

    #[tokio::test]
    async fn mcp_initialize_returns_sse_response() -> anyhow::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let db_path = temp_dir.path().join("restflow.redb");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await?);
        let app = Router::new()
            .nest("/mcp", mcp_router(core))
            .layer(Extension(ApiKeyManager::from_env()));

        let request = HttpRequest::builder()
            .method("POST")
            .uri("/mcp")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#,
            ))?;

        let response = app.oneshot(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.contains("text/event-stream"));

        Ok(())
    }
}
