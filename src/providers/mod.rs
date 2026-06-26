//! Provider clients (one module per source).
//!
//! Each module exposes a `fetch(credential, client) -> anyhow::Result<ProviderUsage>`
//! function that the aggregator calls in parallel via `tokio::join!`.

pub mod zai;
pub mod minimax;
pub mod codex;