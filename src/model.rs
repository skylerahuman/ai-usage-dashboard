//! Unified usage data model — provider-agnostic shape the TUI renders.
//!
//! See `_plans/.../decision-data-model.md` for the design rationale.

use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Zai,
    Minimax,
    Codex,
}

impl Provider {
    pub fn label(&self) -> &'static str {
        match self {
            Provider::Zai => "z.ai",
            Provider::Minimax => "minimax",
            Provider::Codex => "OpenAI Codex",
        }
    }

    pub fn source(&self) -> &'static str {
        match self {
            Provider::Zai => "api.z.ai",
            Provider::Minimax => "api.minimax.io",
            Provider::Codex => "chatgpt.com",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WindowKey {
    FiveHour,
    Weekly,
    Additional(String),
}

impl WindowKey {
    pub fn label(&self) -> String {
        match self {
            WindowKey::FiveHour => "5h".into(),
            WindowKey::Weekly => "Weekly".into(),
            WindowKey::Additional(name) => name.clone(),
        }
    }
}

/// One usage window reported by a provider (e.g. z.ai's 5h TOKENS_LIMIT).
#[derive(Debug, Clone)]
pub struct UsageWindow {
    pub key: WindowKey,
    pub label: String,
    pub used_percent: Option<f64>,
    pub reset_at: Option<i64>,     // epoch seconds
    pub window_seconds: Option<i64>,
    pub used_raw: Option<i64>,     // tokens used this window (z.ai / Codex path)
    pub total_raw: Option<i64>,    // tokens total this window
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum ProviderStatus {
    Live,
    Stale,
    CredentialsMissing,
    Error { message: String },
}

impl ProviderStatus {
    pub fn chip(&self) -> &'static str {
        match self {
            ProviderStatus::Live => "LIVE",
            ProviderStatus::Stale => "STALE",
            ProviderStatus::CredentialsMissing => "NO AUTH",
            ProviderStatus::Error { .. } => "ERROR",
        }
    }
    pub fn is_error(&self) -> bool {
        !matches!(self, ProviderStatus::Live | ProviderStatus::Stale)
    }
}

/// One provider's full snapshot.
#[derive(Debug, Clone)]
pub struct ProviderUsage {
    pub provider: Provider,
    pub label: String,
    pub fetched_at: Option<Instant>,
    pub status: ProviderStatus,
    pub windows: Vec<UsageWindow>,
    pub notes: Vec<String>, // for additional info shown in the panel
}

/// The complete dashboard state.
#[derive(Debug, Clone, Default)]
pub struct Aggregated {
    pub providers: Vec<ProviderUsage>,
    pub last_refresh: Option<Instant>,
    pub next_refresh: Option<Instant>,
    pub auth_source: Option<String>,
}

impl Aggregated {
    /// Sort providers by 5h used_percent desc (None treated as 0).
    pub fn sorted_by_usage(&self) -> Vec<&ProviderUsage> {
        let mut v: Vec<_> = self.providers.iter().collect();
        v.sort_by(|a, b| {
            let key = |p: &ProviderUsage| -> f64 {
                p.windows
                    .iter()
                    .find(|w| matches!(w.key, WindowKey::FiveHour))
                    .and_then(|w| w.used_percent)
                    .unwrap_or(0.0)
            };
            key(b).partial_cmp(&key(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    }
}