//! MiniMax token plan provider.
//!
//! Endpoint: `GET https://api.minimax.io/v1/token_plan/remains`
//! Auth:     `Authorization: Bearer <MINIMAX_CODING_PLAN_KEY>`
//! Caveats:  count fields may be numbers or strings; `remains_time` is a passive
//!           countdown so we don't derive consumption from it.

use crate::config::Credential;
use crate::model::{Provider, ProviderStatus, ProviderUsage, UsageWindow, WindowKey};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::time::Instant;

const URL: &str = "https://api.minimax.io/v1/token_plan/remains";

#[derive(Debug, Deserialize)]
struct MmResp {
    #[serde(default, rename = "model_remains")]
    model_remains: Vec<MmModelRemain>,
    #[serde(default, rename = "base_resp")]
    base_resp: Option<MmBaseResp>,
}

#[derive(Debug, Deserialize)]
struct MmBaseResp {
    #[serde(default, rename = "status_code")] status_code: Option<i64>,
    #[serde(default, rename = "status_msg")] status_msg: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MmModelRemain {
    #[serde(default, rename = "model_name")] model_name: Option<String>,
    #[serde(default, rename = "current_interval_total_count")] current_interval_total: Option<MmCount>,
    #[serde(default, rename = "current_interval_usage_count")] current_interval_usage: Option<MmCount>,
    #[serde(default, rename = "current_interval_remaining_percent")] current_interval_remaining_pct: Option<f64>,
    #[serde(default, rename = "remains_time")] remains_time: Option<i64>,
    #[serde(default, rename = "current_weekly_total_count")] weekly_total: Option<MmCount>,
    #[serde(default, rename = "current_weekly_usage_count")] weekly_usage: Option<MmCount>,
    #[serde(default, rename = "current_weekly_remaining_percent")] weekly_remaining_pct: Option<f64>,
    #[serde(default, rename = "weekly_remains_time")] weekly_remains_time: Option<i64>,
}

/// Field arrives as either number or string (per pi-usage reports).
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum MmCount {
    N(i64),
    S(String),
}
impl MmCount {
    fn as_i64(&self) -> i64 {
        match self {
            MmCount::N(n) => *n,
            MmCount::S(s) => s.parse().unwrap_or(0),
        }
    }
}

pub async fn fetch(cred: &Credential, client: &reqwest::Client) -> anyhow::Result<ProviderUsage> {
    let token = match &cred.token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Ok(ProviderUsage {
                provider: Provider::Minimax,
                label: Provider::Minimax.label().into(),
                fetched_at: None,
                status: ProviderStatus::CredentialsMissing,
                windows: vec![],
                notes: vec!["Set minimax key in ~/.pi/agent/auth.json or MINIMAX_CODING_PLAN_KEY".into()],
            });
        }
    };

    let resp = client
        .get(URL)
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .send()
        .await
        .context("minimax request")?;
    let http_status = resp.status();
    let body: MmResp = resp.json().await.context("minimax parse JSON")?;

    if let Some(br) = &body.base_resp {
        if let Some(code) = br.status_code {
            if code != 0 && code != 1000 {
                return Ok(ProviderUsage {
                    provider: Provider::Minimax,
                    label: Provider::Minimax.label().into(),
                    fetched_at: Some(Instant::now()),
                    status: ProviderStatus::Error {
                        message: br
                            .status_msg
                            .clone()
                            .unwrap_or_else(|| format!("status_code {}", code)),
                    },
                    windows: vec![],
                    notes: vec![format!("HTTP {}", http_status)],
                });
            }
        }
    }

    // Aggregate across models.
    let mut total_5h_used: i64 = 0;
    let mut total_5h_cap: i64 = 0;
    let mut pct_5h_samples: Vec<f64> = Vec::new();
    let mut total_week_used: i64 = 0;
    let mut total_week_cap: i64 = 0;
    let mut pct_week_samples: Vec<f64> = Vec::new();
    let mut max_5h_remains: Option<i64> = None;
    let mut max_week_remains: Option<i64> = None;
    let mut model_names: Vec<String> = Vec::new();

    for m in &body.model_remains {
        if let Some(name) = &m.model_name {
            if !name.is_empty() {
                model_names.push(name.clone());
            }
        }
        if let Some(c) = &m.current_interval_total {
            total_5h_cap += c.as_i64();
        }
        if let Some(c) = &m.current_interval_usage {
            total_5h_used += c.as_i64();
        }
        if let Some(p) = m.current_interval_remaining_pct {
            pct_5h_samples.push(100.0 - p);
        }
        if let Some(c) = &m.weekly_total {
            total_week_cap += c.as_i64();
        }
        if let Some(c) = &m.weekly_usage {
            total_week_used += c.as_i64();
        }
        if let Some(p) = m.weekly_remaining_pct {
            pct_week_samples.push(100.0 - p);
        }
        if let Some(r) = m.remains_time {
            // API returns milliseconds; convert to seconds.
            // Use min across models: that's "when can I use the API again".
            // Different model buckets can have different cycle phases; max
            // would surface the model whose cycle just started (irrelevant
            // to whether the bucket the user actually uses has refilled).
            max_5h_remains = Some(match max_5h_remains {
                None => r / 1000,
                Some(cur) => (r / 1000).min(cur),
            });
        }
        if let Some(r) = m.weekly_remains_time {
            max_week_remains = Some(match max_week_remains {
                None => r / 1000,
                Some(cur) => (r / 1000).min(cur),
            });
        }
    }

    let avg = |v: &[f64]| -> Option<f64> {
        if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) }
    };

    let pct_5h = avg(&pct_5h_samples);
    let pct_week = avg(&pct_week_samples);

    // Use % from API when available; fall back to derived from counts.
    let used_pct_5h = pct_5h.or_else(|| {
        if total_5h_cap > 0 {
            Some((total_5h_used as f64 / total_5h_cap as f64) * 100.0)
        } else {
            None
        }
    });
    let used_pct_week = pct_week.or_else(|| {
        if total_week_cap > 0 {
            Some((total_week_used as f64 / total_week_cap as f64) * 100.0)
        } else {
            None
        }
    });

    let now = chrono::Utc::now().timestamp();
    let windows = vec![
        UsageWindow {
            key: WindowKey::FiveHour,
            label: WindowKey::FiveHour.label(),
            used_percent: used_pct_5h,
            reset_at: max_5h_remains.map(|s| now + s),
            window_seconds: Some(5 * 3600),
            used_raw: Some(total_5h_used),
            total_raw: Some(total_5h_cap),
            raw: serde_json::to_value(&body.model_remains).unwrap_or(serde_json::Value::Null),
        },
        UsageWindow {
            key: WindowKey::Weekly,
            label: WindowKey::Weekly.label(),
            used_percent: used_pct_week,
            reset_at: max_week_remains.map(|s| now + s),
            window_seconds: Some(7 * 24 * 3600),
            used_raw: Some(total_week_used),
            total_raw: Some(total_week_cap),
            raw: serde_json::to_value(&body.model_remains).unwrap_or(serde_json::Value::Null),
        },
    ];

    let notes = if model_names.is_empty() {
        Vec::new()
    } else {
        vec![format!("models: {}", model_names.join(", "))]
    };

    Ok(ProviderUsage {
        provider: Provider::Minimax,
        label: Provider::Minimax.label().into(),
        fetched_at: Some(Instant::now()),
        status: ProviderStatus::Live,
        windows,
        notes,
    })
}