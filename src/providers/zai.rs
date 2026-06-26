//! Z.AI usage provider.
//!
//! Endpoint: `GET https://api.z.ai/api/monitor/usage/quota/limit`
//! Auth:     `Authorization: Bearer <key>`
//! Spec:     https://api.z.ai/api/monitor/usage/quota/limit (live; 1001 without auth)

use crate::config::Credential;
use crate::providers::friendly_http_error;
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
        let _ = resp.text().await;
        return Ok(ProviderUsage {
            provider: Provider::Zai,
            label: Provider::Zai.label().into(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error {
                message: friendly_http_error(status.as_u16()),
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
                message: body.msg.unwrap_or_else(|| friendly_http_error(status.as_u16())),
            },
            windows: vec![],
            notes: vec![],
        });
    }

    // z.ai returns TOKENS_LIMIT entries with (unit, number) combos that vary
    // by plan (e.g. unit=3/num=5 for 5h, unit=6/num=7 for weekly on one plan;
    // unit=6/num=1 for weekly on another). Rather than hard-coding the combos,
    // we collect all token-limit entries and label them by position:
    //   first entry  -> 5h
    //   second entry -> Weekly
    //   any others   -> Additional("tokens")
    // z.ai doesn't always return entries in a stable order, so we sort by
    // window length (estimated from nextResetTime) so the shorter window
    // is always 5h and the longer is weekly.
    let (mut token_limits, time_limits): (Vec<_>, Vec<_>) = body
        .data
        .limits
        .into_iter()
        .partition(|l| l.kind == "TOKENS_LIMIT");
    token_limits.sort_by_key(|l| {
        l.nextResetTime
            .map(|ms| ms / 1000 - chrono::Utc::now().timestamp())
            .unwrap_or(i64::MAX)
    });

    let mut windows = Vec::new();
    for (idx, lim) in token_limits.iter().enumerate() {
        let (key, secs) = match idx {
            0 => (WindowKey::FiveHour, Some(5 * 3600)),
            1 => (WindowKey::Weekly, Some(7 * 24 * 3600)),
            _ => (WindowKey::Additional("tokens".to_string()), None),
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
    let mut notes = Vec::new();
    for lim in time_limits.iter() {
        if let Some(details) = &lim.usageDetails {
            for d in details {
                if let (Some(mc), Some(u)) = (d.modelCode.clone(), d.usage) {
                    notes.push(format!("{}: {} calls", mc, u));
                }
            }
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