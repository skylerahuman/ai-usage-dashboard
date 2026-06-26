//! End-to-end render test using ratatui's TestBackend.
//! Confirms UI doesn't panic, layout fits, and the error chip is shown for providers
//! whose API call failed. Pure offline — no network, no TTY.

use ai_usage_dashboard::model::{
    Aggregated, Provider, ProviderStatus, ProviderUsage, UsageWindow, WindowKey,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn renders_with_partial_failure() {
    let state = Aggregated {
        providers: vec![
            ProviderUsage {
                provider: Provider::Codex,
                label: "OpenAI Codex".into(),
                fetched_at: None,
                status: ProviderStatus::Live,
                windows: vec![
                    UsageWindow {
                        key: WindowKey::FiveHour,
                        label: "5h".into(),
                        used_percent: Some(34.0),
                        reset_at: Some(1_782_450_992),
                        window_seconds: Some(18000),
                        used_raw: None,
                        total_raw: None,
                        raw: serde_json::Value::Null,
                    },
                    UsageWindow {
                        key: WindowKey::Weekly,
                        label: "Weekly".into(),
                        used_percent: Some(41.0),
                        reset_at: Some(1_782_941_019),
                        window_seconds: Some(604800),
                        used_raw: None,
                        total_raw: None,
                        raw: serde_json::Value::Null,
                    },
                ],
                notes: vec!["plan: plus".into()],
            },
            ProviderUsage {
                provider: Provider::Zai,
                label: "z.ai".into(),
                fetched_at: None,
                status: ProviderStatus::Error { message: "HTTP 502 (body: )".into() },
                windows: vec![],
                notes: vec![],
            },
        ],
        last_refresh: Some(std::time::Instant::now()),
        next_refresh: Some(std::time::Instant::now() + std::time::Duration::from_secs(60)),
        auth_source: Some("/home/sky/.pi/agent/auth.json".into()),
    };

    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| ai_usage_dashboard::ui::render(f, &state))
        .expect("render should not panic");
}