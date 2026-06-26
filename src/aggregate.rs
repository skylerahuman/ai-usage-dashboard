use crate::config::Credentials;
use crate::model::{Aggregated, Provider, ProviderStatus, ProviderUsage};
use crate::providers::{codex, minimax, zai};
use anyhow::Result;
use std::time::{Duration, Instant};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

pub async fn refresh(creds: &Credentials, client: &reqwest::Client) -> Aggregated {
    let start = Instant::now();

    let (zai_res, minimax_res, codex_res) = tokio::join!(
        zai::fetch(&creds.zai, client),
        minimax::fetch(&creds.minimax, client),
        codex::fetch(&creds.codex, client),
    );

    let mut providers = Vec::new();
    providers.push(result_or_error(Provider::Zai, zai_res));
    providers.push(result_or_error(Provider::Minimax, minimax_res));
    providers.push(result_or_error(Provider::Codex, codex_res));

    Aggregated {
        providers,
        last_refresh: Some(start),
        next_refresh: Some(start + REFRESH_INTERVAL),
        auth_source: creds.auth_json_path.as_ref().map(|p| p.display().to_string()),
    }
}

fn result_or_error(provider: Provider, res: Result<ProviderUsage>) -> ProviderUsage {
    match res {
        Ok(p) => p,
        Err(e) => ProviderUsage {
            provider,
            label: provider.label().to_string(),
            fetched_at: Some(Instant::now()),
            status: ProviderStatus::Error { message: e.to_string() },
            windows: vec![],
            notes: vec![],
        },
    }
}
