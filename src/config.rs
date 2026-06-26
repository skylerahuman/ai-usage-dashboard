//! Configuration + credential loading.
//!
//! Credential resolution order (per provider):
//!   1. Environment variables
//!   2. `~/.pi/agent/auth.json` (the same file the `pi` coding agent uses)
//!
//! See `_plans/.../decision-credential-source.md`.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Per-provider credential. `account_id` is currently only used by OpenAI Codex.
#[derive(Debug, Clone, Default)]
pub struct Credential {
    pub token: Option<String>,
    pub account_id: Option<String>,
}

/// Resolved credentials for all providers the user has configured.
#[derive(Debug, Default)]
pub struct Credentials {
    pub zai: Credential,
    pub minimax: Credential,
    pub codex: Credential,
    /// Path the auth.json (if any) was loaded from — surfaced in the TUI footer.
    pub auth_json_path: Option<PathBuf>,
}

impl Credentials {
    /// True if at least one provider has a token we can try.
    pub fn any_available(&self) -> bool {
        self.zai.token.is_some() || self.minimax.token.is_some() || self.codex.token.is_some()
    }
}

#[derive(Debug, Default, Deserialize)]
struct AuthFile {
    #[serde(rename = "zai-coding-plan", default)]
    zai_coding_plan: Option<AuthEntry>,
    #[serde(default)]
    minimax: Option<AuthEntry>,
    #[serde(rename = "openai-codex", default)]
    openai_codex: Option<AuthEntry>,
}

/// One entry in `auth.json`. We accept either the `key` form (api_key) or
/// the `access` form (oauth).
#[derive(Debug, Deserialize)]
struct AuthEntry {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    access: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    accountId: Option<String>,
}

/// Load credentials from env + auth.json. `pi_agent_dir` lets tests point at a
/// fixture; pass `None` to use the default `~/.pi/agent`.
pub fn load(pi_agent_dir: Option<&Path>) -> Result<Credentials> {
    let mut creds = Credentials::default();

    // 1) Environment overrides.
    if let Ok(v) = std::env::var("ZAI_API_KEY") {
        if !v.is_empty() {
            creds.zai.token = Some(v);
        }
    }
    if let Ok(v) = std::env::var("MINIMAX_CODING_PLAN_KEY") {
        if !v.is_empty() {
            creds.minimax.token = Some(v);
        }
    }
    if let Ok(v) = std::env::var("OPENAI_CODEX_OAUTH_TOKEN") {
        if !v.is_empty() {
            creds.codex.token = Some(v);
        }
    }
    if let Ok(v) = std::env::var("OPENAI_CODEX_ACCOUNT_ID") {
        if !v.is_empty() {
            creds.codex.account_id = Some(v);
        }
    }

    // 2) auth.json (only fill fields still empty after env pass).
    let dir = pi_agent_dir
        .map(|p| p.to_path_buf())
        .or_else(|| dirs::home_dir().map(|h| h.join(".pi").join("agent")))
        .context("could not determine home dir")?;
    let path = dir.join("auth.json");
    if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let parsed: AuthFile = serde_json::from_str(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        creds.auth_json_path = Some(path.clone());

        if creds.zai.token.is_none() {
            creds.zai.token = parsed.zai_coding_plan.as_ref().and_then(|e| e.key.clone());
        }
        if creds.minimax.token.is_none() {
            creds.minimax.token = parsed.minimax.as_ref().and_then(|e| e.key.clone());
        }
        if creds.codex.token.is_none() {
            if let Some(e) = parsed.openai_codex.as_ref() {
                creds.codex.token = e.access.clone().or_else(|| e.key.clone());
                if creds.codex.account_id.is_none() {
                    creds.codex.account_id = e.account_id.clone().or_else(|| e.accountId.clone());
                }
            }
        }
    }

    Ok(creds)
}