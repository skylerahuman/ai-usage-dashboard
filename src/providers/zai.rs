//! Z.AI usage provider.
//!
//! Endpoint: `GET https://api.z.ai/api/monitor/usage/quota/limit`
//! Auth:     `Authorization: Bearer <key>`
//! Spec:     https://api.z.ai/api/monitor/usage/quota/limit (live; 1001 without auth)

use crate::config::Credential;
use crate::model::{Provider, ProviderStatus, ProviderUsage, UsageWindow, WindowKey};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;

const URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

#[derive(Debug, Deserialize)]
struct ZaiResp {
    #[serde(default)] code: i64,
    #[serde(default)] success: bool,
    #[serde(default)] msg: Option<String>,
    data: ZaiData,
}

#[derive(Debug, Deserialize)]
struct ZaiData {
    limits: Vec<ZaiLimit>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ZaiLimit {
    #[serde(rename = "type")] kind: String,
    #[serde(default)] unit: Option<i64>,
    #[serde(default)] number: Option<i64>,
    #[serde(default)] usage: Option<i64>,
    #[serde(default)] currentValue: Option<i64>,
    #[serde(default)] remaining: Option<i64>,
    #[serde(default)] percentage: Option<f64>,
    #[serde(default)] nextResetTime: Option<i64>,
    #[serde(default)] usageDetails: Option<Vec<ZaiUsageDetail>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ZaiUsageDetail {
    #[serde(default)] modelCode: Option<String>,
    #[serde(default)] usage: Option<i64>,
}

pub async fn fetch(cred: &Credential, client: &reqwest::Client) -> Result<ProviderUsage> {
    let token = match &cred.token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Ok(ProviderUsage {
                provider: Provider::Zai,
                label: Provider::Zai.label().into(),
                fetched_at: None,
                status: ProviderStatus::CredentialsMissing,
                windows: vec![],
                notes: vec!["Set zai-coding-plan key in ~/.pi/agent/auth.json or ZAI_API_KEY".into()],
            });
        }
    };

    let resp = client
        .get(URL)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .context("z.ai request")?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().await.unwrap_or_default();
        return Ok(ProviderUsage {
            provider: Provider::Zai,
            label: Provider::Zai.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error {
                message: format!("HTTP {} (body: {})", status.as_u16(), txt.chars().take(120).collect::<String>()),
            },
            windows: vec![],
            notes: vec![],
        });
    }
    let body: ZaiResp = resp.json().await.context("z.ai parse JSON")?;

    if !body.success || body.code != 200 {
        return Ok(ProviderUsage {
            provider: Provider::Zai,
            label: Provider::Zai.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error {
                message: body
                    .msg
                    .unwrap_or_else(|| format!("z.ai returned code {}", body.code)),
            },
            windows: vec![],
            notes: vec![format!("HTTP {}", status)],
        });
    }

    let mut windows = Vec::new();
    let mut notes = Vec::new();
    for lim in body.data.limits {
        match lim.kind.as_str() {
            "TOKENS_LIMIT" => {
                let key = match (lim.unit, lim.number) {
                    (Some(3), Some(5)) => WindowKey::FiveHour,
                    (Some(6), Some(7)) => WindowKey::Weekly,
                    _ => WindowKey::Additional("tokens".to_string()),
                };
                let secs = match (lim.unit, lim.number) {
                    (Some(3), Some(5)) => Some(5 * 3600),
                    (Some(6), Some(7)) => Some(7 * 24 * 3600),
                    _ => None,
                };
                windows.push(UsageWindow {
                    label: key.label(),
                    key,
                    used_percent: lim.percentage,
                    reset_at: lim.nextResetTime.map(|ms| ms / 1000),
                    window_seconds: secs,
                    used_raw: lim.currentValue,
                    total_raw: lim.usage,
                    raw: serde_json::to_value(&lim).unwrap_or(serde_json::Value::Null),
                });
            }
            "TIME_LIMIT" => {
                if let Some(details) = lim.usageDetails {
                    for d in details {
                        if let (Some(mc), Some(u)) = (d.modelCode, d.usage) {
                            notes.push(format!("{}: {} calls", mc, u));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(ProviderUsage {
        provider: Provider::Zai,
        label: Provider::Zai.label().into(),
        fetched_at: Some(Instant::now()),
        status: ProviderStatus::Live,
        windows,
        notes,
    })
}

// Helper used by tests / debugging.
#[allow(dead_code)]
pub fn parse_response(_raw: &str) -> Result<()> {
    Err(anyhow!("not used directly"))
}