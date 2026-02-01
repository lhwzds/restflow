use super::ApiKeyManager;
use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Claims {
    sub: Option<String>,
    exp: Option<usize>,
}

pub async fn auth_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if !path.starts_with("/api") || path.starts_with("/api/public") {
        return next.run(req).await;
    }

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

    let token = match extract_bearer(req.headers().get(axum::http::header::AUTHORIZATION)) {
        Some(token) => token,
        None => return unauthorized(),
    };

    if let Some(manager) = req.extensions().get::<Arc<ApiKeyManager>>() {
        if manager.validate_key(&token).is_some() {
            return next.run(req).await;
        }
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

fn extract_bearer(header: Option<&HeaderValue>) -> Option<String> {
    let value = header?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(|token| token.trim().to_string())
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response()
}
