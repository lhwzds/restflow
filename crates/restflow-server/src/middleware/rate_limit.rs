use axum::{
    Json,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<RateLimitState>>,
}

struct RateLimitState {
    limit: u64,
    window: Duration,
    hits: VecDeque<Instant>,
}

impl RateLimiter {
    pub fn new(limit_per_minute: Option<u64>) -> Option<Self> {
        limit_per_minute.map(|limit| Self {
            state: Arc::new(Mutex::new(RateLimitState {
                limit,
                window: Duration::from_secs(60),
                hits: VecDeque::new(),
            })),
        })
    }

    fn allow(&self) -> bool {
        let mut state = self.state.lock().expect("rate limit lock");
        let now = Instant::now();
        while let Some(front) = state.hits.front() {
            if now.duration_since(*front) > state.window {
                state.hits.pop_front();
            } else {
                break;
            }
        }

        if state.hits.len() as u64 >= state.limit {
            return false;
        }

        state.hits.push_back(now);
        true
    }
}

pub async fn rate_limit_middleware(req: Request, next: Next) -> Response {
    if let Some(limiter) = req.extensions().get::<Option<RateLimiter>>() {
        if let Some(limiter) = limiter {
            if !limiter.allow() {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": "Rate limit exceeded"})),
                )
                    .into_response();
            }
        }
    }

    next.run(req).await
}
