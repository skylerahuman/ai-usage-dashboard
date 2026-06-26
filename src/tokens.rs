//! Read token usage from pi's session JSONL files.
//!
//! pi writes a `usage` block on every assistant message it sees, with
//! `input`, `output`, `cacheRead`, `cacheWrite`, `totalTokens`, `cost`,
//! and the model name. We aggregate those into per-model rows so the
//! dashboard can show real numbers (not just the live snapshot from the
//! provider APIs).
//!
//! Schema is the `v:1` jsonl format pi uses:
//!   { v, t (ms epoch), sid, branch, entry: { role, content, model, usage, ... } }

use crate::model::Provider;
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub enum TokenWindow {
    All,
    Last24h,
    Last7d,
}

impl TokenWindow {
    pub fn label(&self) -> &'static str {
        match self {
            TokenWindow::All => "all-time",
            TokenWindow::Last24h => "last 24h",
            TokenWindow::Last7d => "last 7d",
        }
    }
    fn since_ms(&self) -> Option<i64> {
        let now = chrono::Utc::now().timestamp_millis();
        match self {
            TokenWindow::All => None,
            TokenWindow::Last24h => Some(now - 24 * 3600 * 1000),
            TokenWindow::Last7d => Some(now - 7 * 24 * 3600 * 1000),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Entry {
    role: Option<String>,
    model: Option<String>,
    timestamp: Option<i64>,
    usage: Option<Usage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Usage {
    input: i64,
    output: i64,
    #[serde(rename = "cacheRead")]
    cache_read: i64,
    #[serde(rename = "cacheWrite")]
    cache_write: i64,
    #[serde(rename = "totalTokens")]
    total: i64,
    cost: Option<Cost>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Cost {
    total: f64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Record {
    v: Option<i64>,
    t: Option<i64>,
    entry: Entry,
}

impl Default for Record {
    fn default() -> Self {
        Record { v: None, t: None, entry: Entry::default() }
    }
}

#[derive(Debug, Clone)]
pub struct TokenRow {
    pub model: String,
    pub provider: Provider,
    pub msgs: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub total: i64,
    pub cost: f64,
}

#[derive(Debug)]
pub struct TokenSummary {
    pub window: TokenWindow,
    pub rows: Vec<TokenRow>,
}

impl TokenSummary {
    pub fn collect(window: TokenWindow) -> Self {
        let mut rows = match scan(window) {
            Ok(rows) => rows,
            Err(_) => Vec::new(),
        };
        // Always show one placeholder row per provider, so the user can see
        // "this is where z.ai / codex tokens will appear once I use them".
        for (model, provider) in [
            ("glm-*", Provider::Zai),
            ("minimax-*", Provider::Minimax),
            ("gpt-*", Provider::Codex),
        ] {
            if !rows.iter().any(|r| r.provider == provider) {
                rows.push(TokenRow {
                    model: model.into(),
                    provider,
                    msgs: 0,
                    input: 0,
                    output: 0,
                    cache_read: 0,
                    total: 0,
                    cost: 0.0,
                });
            }
        }
        rows.sort_by(|a, b| {
            // Healthy providers first (non-zero total), then placeholders.
            b.total.cmp(&a.total).then_with(|| provider_order(a.provider).cmp(&provider_order(b.provider)))
        });
        TokenSummary { window, rows }
    }
}

fn provider_order(p: Provider) -> u8 {
    match p {
        Provider::Codex => 0,
        Provider::Minimax => 1,
        Provider::Zai => 2,
    }
}

/// Heuristic model → provider mapping. Falls back to None → kept as a synthetic "unknown" provider.
fn classify(model: &str) -> Provider {
    let m = model.to_lowercase();
    if m.starts_with("glm-") || m.contains("z.ai") || m.contains("zai") {
        Provider::Zai
    } else if m.starts_with("minimax") || m.starts_with("minimax") {
        Provider::Minimax
    } else if m.starts_with("gpt-") || m.contains("codex") || m.contains("openai") {
        Provider::Codex
    } else {
        Provider::Zai
    }
}

fn scan(window: TokenWindow) -> Result<Vec<TokenRow>> {
    let dirs = session_dirs();
    let since = window.since_ms();
    let mut by_model: BTreeMap<String, TokenRow> = BTreeMap::new();
    for dir in dirs {
        walk(&dir, since, &mut by_model)?;
    }
    let mut rows: Vec<TokenRow> = by_model.into_values().collect();
    rows.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.model.cmp(&b.model)));
    Ok(rows)
}

fn session_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for sub in [".pi/sessions", ".pi/sessions/archive", ".pi/agent/sessions"] {
            let p = home.join(sub);
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out
}

fn walk(dir: &std::path::Path, since: Option<i64>, out: &mut BTreeMap<String, TokenRow>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, since, out)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(s) = std::fs::read_to_string(&path) else { continue; };
        for ln in s.lines() {
            let Ok(rec) = serde_json::from_str::<Record>(ln) else { continue; };
            let entry = rec.entry;
            if entry.role.as_deref() != Some("assistant") {
                continue;
            }
            let ts = rec.t.or(entry.timestamp);
            if let (Some(since_ms), Some(ts)) = (since, ts) {
                if ts < since_ms {
                    continue;
                }
            }
            let (Some(model), Some(usage)) = (entry.model.as_ref(), entry.usage.as_ref()) else { continue; };
            let cost = usage.cost.as_ref().map(|c| c.total).unwrap_or(0.0);
            let row = out.entry(model.clone()).or_insert_with(|| TokenRow {
                model: model.clone(),
                provider: classify(model),
                msgs: 0,
                input: 0,
                output: 0,
                cache_read: 0,
                total: 0,
                cost: 0.0,
            });
            row.msgs += 1;
            row.input += usage.input;
            row.output += usage.output;
            row.cache_read += usage.cache_read;
            row.total += usage.total;
            row.cost += cost;
        }
    }
    Ok(())
}