use ai_usage_dashboard::{aggregate, config, model, ui};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let oneshot = args.iter().any(|a| a == "--once" || a == "--json");

    let creds = config::load(None)?;
    let client = build_client()?;

    if oneshot {
        let state = aggregate::refresh(&creds, &client).await;
        print_once(&state);
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, creds, client).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("{err:?}");
    }
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    creds: config::Credentials,
    client: reqwest::Client,
) -> Result<()> {
    let mut state = aggregate::refresh(&creds, &client).await;
    let mut tick = tokio::time::interval(Duration::from_millis(250));

    loop {
        terminal.draw(|f| ui::render(f, &state))?;

        tokio::select! {
            _ = tick.tick() => {
                if event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => break,
                                KeyCode::Char('r') => state = aggregate::refresh(&creds, &client).await,
                                _ => {}
                            }
                        }
                    }
                }
                if state.next_refresh.map(|i| i <= std::time::Instant::now()).unwrap_or(false) {
                    state = aggregate::refresh(&creds, &client).await;
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

/// Build the reqwest client, optionally loading an extra CA bundle.
///
/// Some hosts (notably machines running `llmtrim`) ship a self-signed CA at
/// `~/.llmtrim/ca.pem` so their HTTP interception works, but rustls ignores
/// the Node-style `NODE_EXTRA_CA_CERTS` env var. We try a few well-known
/// locations so the dashboard works on hosts where the system trust store
/// is broken. Disable with `AI_USAGE_DASHBOARD_NO_EXTRA_CA=1`.
fn build_client() -> Result<reqwest::Client> {
    use anyhow::Context;

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("ai-usage-dashboard/0.1.0");

    if std::env::var("AI_USAGE_DASHBOARD_NO_EXTRA_CA").ok().as_deref() != Some("1") {
        let candidates: [Option<String>; 5] = [
            std::env::var("AI_USAGE_DASHBOARD_CA_FILE").ok(),
            Some("/home/sky/.llmtrim/ca.pem".to_string()),
            std::env::var("NODE_EXTRA_CA_CERTS").ok(),
            dirs::home_dir().map(|h| h.join(".config/llmtrim/ca.pem").display().to_string()),
            dirs::home_dir().map(|h| h.join(".pi-trim/ca.pem").display().to_string()),
        ];
        for path in candidates.into_iter().flatten() {
            if let Ok(pem) = std::fs::read(&path) {
                if let Ok(cert) = reqwest::Certificate::from_pem(&pem) {
                    builder = builder.add_root_certificate(cert);
                    eprintln!("[ai-usage-dashboard] loaded extra CA from {}", path);
                    break;
                }
            }
        }
    }

    Ok(builder.build().context("build reqwest client")?)
}

fn print_once(state: &model::Aggregated) {
    println!("ai-usage-dashboard");
    for p in state.sorted_by_usage() {
        println!("{} [{}]", p.label, p.status.chip());
        if let model::ProviderStatus::Error { message } = &p.status {
            println!("  error: {message}");
        }
        for w in &p.windows {
            let pct = w.used_percent.map(|p| format!("{p:.1}%")).unwrap_or_else(|| "n/a".into());
            let counts = match (w.used_raw, w.total_raw) {
                (Some(u), Some(t)) if t > 0 => format!(" {u}/{t}"),
                (Some(u), _) => format!(" {u}"),
                _ => String::new(),
            };
            println!("  {:<8} {:>8}{}", w.label, pct, counts);
        }
        for n in &p.notes {
            println!("  note: {n}");
        }
    }
}
