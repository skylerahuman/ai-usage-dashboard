//! OpenAI Codex usage provider.
//!
//! Endpoint: `GET https://chatgpt.com/backend-api/wham/usage`
//! Auth:     `Authorization: Bearer <oauth_access_token>`
//!            `ChatGPT-Account-Id: <account_id>` (optional)
//! Spec:     Reverse-engineered from `pi-vault/pi-usage` openai-codex provider.

use crate::config::Credential;
use crate::model::{Provider, ProviderStatus, ProviderUsage, UsageWindow, WindowKey};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::time::Instant;

const URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Deserialize)]
struct CodexResp {
    #[serde(default)] rate_limit: Option<RateLimit>,
    #[serde(default)] additional_rate_limits: Option<Vec<RateLimitEntry>>,
    #[serde(default)] credits: Option<Credits>,
}

#[derive(Debug, Deserialize)]
struct Credits {
    #[serde(default, rename = "has_credits")] has_credits: Option<bool>,
    #[serde(default)] unlimited: Option<bool>,
    #[serde(default, rename = "balance")] balance: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    #[serde(default)] primary_window: Option<Window>,
    #[serde(default)] secondary_window: Option<Window>,
}

#[derive(Debug, Deserialize)]
struct RateLimitEntry {
    #[serde(default)] limit_name: Option<String>,
    #[serde(default)] metered_feature: Option<String>,
    #[serde(default)] rate_limit: Option<RateLimit>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Window {
    #[serde(default)] limit_window_seconds: Option<i64>,
    #[serde(default)] used_percent: Option<f64>,
    #[serde(default)] reset_at: Option<i64>,
}

pub async fn fetch(cred: &Credential, client: &reqwest::Client) -> anyhow::Result<ProviderUsage> {
    let token = match &cred.token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Ok(ProviderUsage {
                provider: Provider::Codex,
                label: Provider::Codex.label().into(),
                fetched_at: None,
                status: ProviderStatus::CredentialsMissing,
                windows: vec![],
                notes: vec!["Set openai-codex.access in ~/.pi/agent/auth.json or OPENAI_CODEX_OAUTH_TOKEN".into()],
            });
        }
    };

    let mut req = client
        .get(URL)
        .bearer_auth(token)
        .header("Accept", "application/json");
    if let Some(aid) = cred.account_id.as_ref().filter(|s| !s.is_empty()) {
        req = req.header("ChatGPT-Account-Id", aid);
    }

    let resp = req.send().await.context("codex request")?;
    let http_status = resp.status();
    let headers = resp.headers().clone();
    if !http_status.is_success() && !matches!(http_status.as_u16(), 401 | 403 | 429) {
        let txt = resp.text().await.unwrap_or_default();
        return Ok(ProviderUsage {
            provider: Provider::Codex,
            label: Provider::Codex.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error { message: format!("HTTP {} (body: {})", http_status.as_u16(), txt.chars().take(120).collect::<String>()) },
            windows: vec![],
            notes: vec![],
        });
    }
    let body_text = resp.text().await.unwrap_or_default();
    if http_status.is_success() && std::env::var("AI_USAGE_DASHBOARD_DEBUG_CODEX").ok().as_deref() == Some("1") {
        eprintln!("[codex body] {}", body_text);
    }
    let body: CodexResp = serde_json::from_str(&body_text)
        .map_err(|e| {
            eprintln!("[codex parse] {e} body(first 200)={}", body_text.chars().take(200).collect::<String>());
            anyhow::anyhow!("codex parse JSON: {e}")
        })?;

    if http_status.as_u16() == 401 || http_status.as_u16() == 403 {
        return Ok(ProviderUsage {
            provider: Provider::Codex,
            label: Provider::Codex.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error { message: "auth failed — re-login required".into() },
            windows: vec![],
            notes: vec![format!("HTTP {}", http_status)],
        });
    }
    if http_status.as_u16() == 429 {
        let retry = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .map(|s| format!("retry-after: {}s", s))
            .unwrap_or_else(|| "rate limited".into());
        return Ok(ProviderUsage {
            provider: Provider::Codex,
            label: Provider::Codex.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error { message: retry },
            windows: vec![],
            notes: vec!["HTTP 429".into()],
        });
    }
    if !http_status.is_success() {
        return Ok(ProviderUsage {
            provider: Provider::Codex,
            label: Provider::Codex.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error { message: format!("HTTP {}", http_status) },
            windows: vec![],
            notes: vec!["live source unavailable".into()],
        });
    }

    let mut windows: Vec<UsageWindow> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    if let Some(c) = &body.credits {
        let bal = c.balance.clone().unwrap_or_default();
        let unlim = c.unlimited.unwrap_or(false);
        let has = c.has_credits.unwrap_or(false);
        if unlim {
            notes.push("plan: unlimited".into());
        } else if has {
            notes.push(format!("credits balance: {}", bal));
        }
    }

    if let Some(rl) = body.rate_limit {
        if let Some(w) = rl.primary_window {
            windows.push(classify_window(w, "primary".to_string()));
        }
        if let Some(w) = rl.secondary_window {
            windows.push(classify_window(w, "secondary".to_string()));
        }
    }
    for (i, entry) in body.additional_rate_limits.as_deref().unwrap_or(&[]).iter().enumerate() {
        let prefix = entry
            .limit_name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| entry.metered_feature.clone().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| format!("Additional {}", i + 1));
        if let Some(rl) = &entry.rate_limit {
            if let Some(w) = &rl.primary_window {
                windows.push(classify_window(w.clone(), format!("{}:primary", prefix)));
            }
            if let Some(w) = &rl.secondary_window {
                windows.push(classify_window(w.clone(), format!("{}:secondary", prefix)));
            }
        }
        notes.push(format!("add'l limit: {}", prefix));
    }

    Ok(ProviderUsage {
        provider: Provider::Codex,
        label: Provider::Codex.label().into(),
        fetched_at: Some(Instant::now()),
        status: ProviderStatus::Live,
        windows,
        notes,
    })
}

fn classify_window(w: Window, name: String) -> UsageWindow {
    let secs = w.limit_window_seconds.unwrap_or(0);
    let key = match secs {
        s if s == 5 * 3600 => WindowKey::FiveHour,
        s if s == 7 * 24 * 3600 => WindowKey::Weekly,
        _ => WindowKey::Additional(name),
    };
    UsageWindow {
        label: key.label(),
        key,
        used_percent: w.used_percent,
        reset_at: w.reset_at,
        window_seconds: w.limit_window_seconds,
        used_raw: None,
        total_raw: None,
        raw: serde_json::to_value(&w).unwrap_or(serde_json::Value::Null),
    }
}