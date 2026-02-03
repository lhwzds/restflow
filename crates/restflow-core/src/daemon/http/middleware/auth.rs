use crate::daemon::http::ApiError;
use axum::{
    http::{header, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub async fn require_api_key<B>(
    req: Request<B>,
    next: Next<B>,
    api_key: Option<String>,
) -> Response {
    let Some(expected) = api_key else {
        return ApiError::unauthorized("API key is not configured").into_response();
    };

    let Some(value) = req.headers().get(header::AUTHORIZATION) else {
        return ApiError::unauthorized("Missing Authorization header").into_response();
    };

    let Ok(header_value) = value.to_str() else {
        return ApiError::unauthorized("Invalid Authorization header").into_response();
    };

    let token = header_value.strip_prefix("Bearer ").unwrap_or(header_value);
    if token != expected {
        return ApiError::unauthorized("Invalid API key").into_response();
    }

    next.run(req).await
}
