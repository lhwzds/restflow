use crate::daemon::http::HttpConfig;
use axum::http::HeaderValue;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

pub fn build_cors_layer(config: &HttpConfig) -> CorsLayer {
    let mut layer = CorsLayer::new().allow_methods(Any).allow_headers(Any);

    if config.cors_origins.is_empty() || config.cors_origins.iter().any(|o| o == "*") {
        layer = layer.allow_origin(Any);
        return layer;
    }

    let origins: Vec<HeaderValue> = config
        .cors_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();

    if origins.is_empty() {
        layer.allow_origin(Any)
    } else {
        layer.allow_origin(AllowOrigin::list(origins))
    }
}
