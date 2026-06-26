//! Provider clients (one module per source).
//!
//! Each module exposes a `fetch(credential, client) -> anyhow::Result<ProviderUsage>`
//! function that the aggregator calls in parallel via `tokio::join!`.

pub mod zai;
pub mod minimax;
pub mod codex;

/// Map an HTTP status code to a one-line user-facing error.
/// Anything we don't recognize falls back to "Unreachable (HTTP NNN)".
pub(crate) fn friendly_http_error(status: u16) -> String {
    match status {
        401 | 403 => "Auth Failed (re-login required)".into(),
        404 => "Endpoint Not Found".into(),
        408 => "Request Timed Out".into(),
        429 => "Rate Limited".into(),
        500 => "Internal Server Error".into(),
        502 | 503 | 504 => "Service Unavailable".into(),
        _ => format!("Unreachable (HTTP {})", status),
    }
}