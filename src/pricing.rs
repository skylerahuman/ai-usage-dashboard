//! Public pricing for PAYG (pay-as-you-go) API rates across the models we
//! might see in pi session history. Used to compute an *estimated* cost
//! when the provider-reported `cost` is $0 (because the user is on a flat
//! subscription like the z.ai Coding Plan or the minimax Token Plan).
//!
//! Rates are per 1 MILLION tokens, in USD.
//!
//! Sources (verified 2026-06-25):
//!   - z.ai GLM-5.2: docs.z.ai/guides/overview/pricing
//!   - minimax M-series: platform.minimax.io/docs/guides/pricing-paygo
//!   - OpenAI GPT-5.x: developers.openai.com/api/docs/pricing
//!   - Anthropic Claude: platform.claude.com/docs/en/about-claude/pricing
//!   - DeepSeek: api-docs.deepseek.com/quick_start/pricing

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelPricing {
    pub input: f64,
    pub cached: Option<f64>,
    pub output: f64,
}

impl ModelPricing {
    pub const fn new(input: f64, output: f64) -> Self {
        Self { input, cached: None, output }
    }
    pub const fn with_cache(input: f64, cached: f64, output: f64) -> Self {
        Self { input, cached: Some(cached), output }
    }
}

fn table() -> HashMap<&'static str, ModelPricing> {
    let mut t = HashMap::new();

    // z.ai / GLM Coding Plan users want the PAYG rate for comparison
    t.insert("glm-5.2",        ModelPricing::new(0.55, 1.10));
    t.insert("glm-5",          ModelPricing::new(0.55, 1.10));
    t.insert("glm-5-turbo",    ModelPricing::new(0.20, 0.40));
    t.insert("glm-4.7",        ModelPricing::new(0.10, 0.20));
    t.insert("glm-4.6",        ModelPricing::new(0.10, 0.20));
    t.insert("glm-4.5",        ModelPricing::new(0.10, 0.20));
    t.insert("glm-4.5-air",    ModelPricing::new(0.01, 0.01));
    t.insert("glm-4-air",      ModelPricing::new(0.01, 0.01));
    t.insert("glm-4.5-air:free", ModelPricing::new(0.0, 0.0));
    t.insert("z-ai/glm-5.2",         ModelPricing::new(0.55, 1.10));
    t.insert("z-ai/glm-4.5-air",     ModelPricing::new(0.01, 0.01));
    t.insert("z-ai/glm-4.5-air:free",ModelPricing::new(0.0, 0.0));

    // minimax (uses "minimax" or "MiniMax" prefix in your history)
    t.insert("minimax-m3",             ModelPricing::with_cache(0.30, 0.06, 1.20));
    t.insert("minimax-m2.7",           ModelPricing::with_cache(0.15, 0.06, 0.60));
    t.insert("minimax-m2.7-highspeed", ModelPricing::with_cache(0.30, 0.06, 1.20));

    // OpenAI / Codex (pi stores with dashes: gpt-5-5, with dots in keys: gpt-5.5)
    t.insert("gpt-5.5",           ModelPricing::with_cache(5.00, 0.50, 30.00));
    t.insert("gpt-5.5-pro",       ModelPricing::with_cache(15.00, 1.50, 90.00));
    t.insert("gpt-5.4",           ModelPricing::with_cache(2.50, 0.25, 15.00));
    t.insert("gpt-5.4-mini",      ModelPricing::with_cache(0.75, 0.075, 4.50));
    t.insert("gpt-5",             ModelPricing::with_cache(1.25, 0.125, 10.00));
    t.insert("gpt-5-mini",        ModelPricing::with_cache(0.25, 0.025, 2.00));
    t.insert("gpt-5.3-codex-spark",ModelPricing::with_cache(0.50, 0.05, 3.00));
    t.insert("openai/gpt-5.4-mini", ModelPricing::with_cache(0.75, 0.075, 4.50));

    // Anthropic Claude
    t.insert("claude-opus-4.8",            ModelPricing::with_cache(5.00, 0.50, 25.00));
    t.insert("claude-opus-4.5",            ModelPricing::with_cache(5.00, 0.50, 25.00));
    t.insert("claude-sonnet-4.6",          ModelPricing::with_cache(3.00, 0.30, 15.00));
    t.insert("claude-sonnet-4.5",          ModelPricing::with_cache(3.00, 0.30, 15.00));
    t.insert("claude-haiku-4.5",           ModelPricing::with_cache(1.00, 0.10, 5.00));

    // DeepSeek
    t.insert("deepseek-v4-pro",  ModelPricing::with_cache(0.435, 0.003625, 0.87));
    t.insert("deepseek-v4-flash",ModelPricing::new(0.14, 0.28));

    // OpenRouter free router
    t.insert("openrouter/free",  ModelPricing::new(0.0, 0.0));

    t
}

/// Generate lookup candidate keys from a raw model name.
/// Handles: case, "minimax/m2.7" paths, OpenAI's "gpt-5-5" dash form vs our
/// "gpt-5.5" dot form, Anthropic's "claude-opus-4-8-20250901" date suffixes,
/// minimax "-highspeed" / "minimax-..." prefixes, "z-ai/glm-..." paths.
fn candidates(model: &str) -> Vec<String> {
    let lower = model.to_lowercase();
    let mut out: Vec<String> = Vec::new();

    // 1. original
    out.push(lower.clone());

    // 2. last path segment (handles "z-ai/glm-5.2" -> "glm-5.2")
    if let Some(last) = lower.rsplit('/').next() {
        if !out.contains(&last.to_string()) {
            out.push(last.to_string());
        }
    }

    // 3. strip ":free" suffix (z.ai)
    let no_free = lower.trim_end_matches(":free").to_string();
    if !out.contains(&no_free) {
        out.push(no_free.clone());
    }
    if let Some(last) = no_free.rsplit('/').next() {
        let v = last.to_string();
        if !out.contains(&v) { out.push(v); }
    }

    // 4. strip "-highspeed" suffix (minimax M2.7-highspeed)
    let no_speed = lower.trim_end_matches("-highspeed").to_string();
    if !out.contains(&no_speed) {
        out.push(no_speed.clone());
    }

    // 5. strip "-YYYYMMDD" date suffix (Anthropic snapshot names)
    let stripped_date = strip_date_suffix(&lower);
    if stripped_date != lower && !out.contains(&stripped_date) {
        out.push(stripped_date.clone());
    }

    // 6. dash-to-dot variants for major version numbers
    //    e.g. "gpt-5-5" -> "gpt-5.5", "claude-opus-4-8" -> "claude-opus-4.8"
    for n in 3..=9 {
        let dash_form = format!("-{}-", n);
        let dot_form = format!("-{}.", n);
        let dotted = lower.replace(&dash_form, &dot_form);
        if dotted != lower && !out.contains(&dotted) {
            out.push(dotted);
        }
    }

    out
}

fn strip_date_suffix(s: &str) -> String {
    // Look for "-YYYYMMDD" or "-YYYY-MM-DD" anchored at end.
    if s.len() < 9 { return s.to_string(); }
    let bytes = s.as_bytes();
    let n = bytes.len();
    // -YYYYMMDD (8 digits, preceded by '-')
    if n >= 9 && bytes[n - 9] == b'-' && bytes[n - 8..n].iter().all(|b| b.is_ascii_digit()) {
        return s[..n - 9].to_string();
    }
    // -YYYY-MM-DD (10 chars, all digits except '-' at positions n-6 and n-3)
    if n >= 11
        && bytes[n - 11] == b'-'
        && bytes[n - 10..n - 8].iter().all(|b| b.is_ascii_digit())
        && bytes[n - 8] == b'-'
        && bytes[n - 7..n - 5].iter().all(|b| b.is_ascii_digit())
        && bytes[n - 5] == b'-'
        && bytes[n - 4..n].iter().all(|b| b.is_ascii_digit())
    {
        return s[..n - 11].to_string();
    }
    s.to_string()
}

pub fn lookup(model: &str) -> Option<ModelPricing> {
    let t = table();
    for c in candidates(model) {
        if let Some(p) = t.get(c.as_str()) { return Some(*p); }
        // Try stripping the path again on this candidate.
        if let Some(last) = c.rsplit('/').next() {
            if let Some(p) = t.get(last) { return Some(*p); }
        }
        // Prefix match (longest key wins).
        let mut best: Option<(&str, ModelPricing)> = None;
        for (k, v) in t.iter() {
            if c.starts_with(k) && (best.is_none() || k.len() > best.unwrap().0.len()) {
                best = Some((k, *v));
            }
        }
        if let Some((_, v)) = best { return Some(v); }
    }
    None
}

pub fn estimated_cost(model: &str, input: i64, cached: i64, output: i64) -> Option<f64> {
    let p = lookup(model)?;
    let cached_rate = p.cached.unwrap_or(p.input * 0.10);
    let cost =
        (input as f64 / 1_000_000.0) * p.input
      + (cached as f64 / 1_000_000.0) * cached_rate
      + (output as f64 / 1_000_000.0) * p.output;
    Some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookup_basic() {
        assert!(lookup("gpt-5.5").is_some());
        assert!(lookup("GPT-5.5").is_some());
        assert!(lookup("gpt-5-5").is_some(), "dash form should also resolve");
    }
    #[test]
    fn lookup_claude_with_date() {
        // pi stores Anthropic snapshot names like "claude-opus-4-8-20250901"
        assert!(lookup("claude-opus-4-8-20250901").is_some());
        assert!(lookup("claude-sonnet-4-5-20250929").is_some());
        assert!(lookup("claude-opus-4.8").is_some());
    }
    #[test]
    fn lookup_minimax_with_prefix() {
        // pi stores with "MiniMax-" prefix
        assert!(lookup("MiniMax-M3").is_some());
        assert!(lookup("MiniMax-M2.7-highspeed").is_some());
    }
    #[test]
    fn lookup_zai_path() {
        assert!(lookup("z-ai/glm-5.2").is_some());
        assert!(lookup("z-ai/glm-4.5-air:free").is_some());
    }
    #[test]
    fn cost_calculation() {
        // 1M input at $0.55/M, 4M cached at $0.05/M, 0.5M output at $1.10/M
        // glm-5.2: input $0.55, no cache, output $1.10
        let c = estimated_cost("glm-5.2", 1_000_000, 0, 500_000).unwrap();
        assert!((c - (0.55 + 0.55)).abs() < 0.001, "got {c}");
    }
}
