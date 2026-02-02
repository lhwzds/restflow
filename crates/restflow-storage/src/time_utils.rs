use chrono::Utc;

/// Get current timestamp in milliseconds.
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Get current timestamp in nanoseconds.
pub fn now_ns() -> i64 {
    Utc::now().timestamp_nanos_opt().unwrap_or(0)
}
