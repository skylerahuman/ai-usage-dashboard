use ai_usage_dashboard::{aggregate, config, model, tokens::TokenWindow, ui};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
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
    let token_window = parse_token_window(&args);

    let creds = config::load(None)?;
    let client = build_client()?;

    if oneshot {
        let state = aggregate::refresh(&creds, &client).await;
        let summary = ai_usage_dashboard::tokens::TokenSummary::collect(token_window);
        print_once(&state, &summary);
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, creds, client, token_window).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("{err:?}");
    }
    Ok(())
}

fn parse_token_window(args: &[String]) -> TokenWindow {
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == "--since" {
            if let Some(v) = args.get(idx + 1) {
                return match v.as_str() {
                    "24h" | "1d" => TokenWindow::Last24h,
                    "7d" | "1w" => TokenWindow::Last7d,
                    "all" | "0" => TokenWindow::All,
                    _ => TokenWindow::All,
                };
            }
        }
        idx += 1;
    }
    TokenWindow::All
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    creds: config::Credentials,
    client: reqwest::Client,
    token_window: TokenWindow,
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<crossterm::event::KeyEvent>(16);

    // Dedicated blocking thread for input — `crossterm::event::read` blocks
    // until a key arrives, which is the only reliable way to deliver keys
    // on Linux terminals when mixing with tokio. The earlier code polled
    // with a zero timeout inside a tokio select arm, which dropped keys
    // on many setups (including the one this project was developed on).
    std::thread::spawn(move || {
        loop {
            // `read()` returns immediately on Resize events; we only forward keys.
            match event::read() {
                Ok(Event::Key(k)) => {
                    if tx.blocking_send(k).is_err() { break; }
                }
                Ok(Event::Resize(_, _)) => continue,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    });

    let mut state = aggregate::refresh(&creds, &client).await;
    let mut summary = ai_usage_dashboard::tokens::TokenSummary::collect(token_window);
    let mut tick = tokio::time::interval(Duration::from_millis(250));

    loop {
        terminal.draw(|f| ui::render(f, &state, Some(&summary)))?;

        tokio::select! {
            maybe_key = rx.recv() => {
                let Some(key) = maybe_key else { break; };
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        state = aggregate::refresh(&creds, &client).await;
                        summary = ai_usage_dashboard::tokens::TokenSummary::collect(token_window);
                    }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                if state.next_refresh.map(|i| i <= std::time::Instant::now()).unwrap_or(false) {
                    state = aggregate::refresh(&creds, &client).await;
                    summary = ai_usage_dashboard::tokens::TokenSummary::collect(token_window);
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

fn print_once(state: &model::Aggregated, summary: &ai_usage_dashboard::tokens::TokenSummary) {
    println!("ai-usage-dashboard");
    if !summary.rows.is_empty() {
        println!();
        println!("tokens ({})", summary.window.label());
        println!("{:<22} {:>6} {:>14} {:>14} {:>14} {:>14} {:>10}", "model", "msgs", "input", "output", "cached", "total", "cost");
        for r in &summary.rows {
            println!("{:<22} {:>6} {:>14} {:>14} {:>14} {:>14} {:>10}",
                truncate(&r.model, 22), r.msgs, r.input, r.output, r.cache_read, r.total, format!("${:.4}", r.cost));
        }
    }
    println!();
    for p in state.sorted_by_usage() {
        println!("{} [{}]", p.label, p.status.chip());
        if let model::ProviderStatus::Error { message } = &p.status {
            println!("  error: {message}");
        }
        for w in &p.windows {
            let pct = w.used_percent.map(|p| format!("{p:.1}%")).unwrap_or_else(|| "n/a".into());
            let counts = match (w.used_raw, w.total_raw) {
                (Some(_u), Some(t)) if t > 0 && _u > 0 => format!(" {_u}/{t}"),
                (Some(_), Some(t)) if t > 0 => format!(" 0/{t}"),
                (Some(u), _) if u > 0 => format!(" {u}"),
                _ => String::new(),
            };
            println!("  {:<8} {:>8}{}", w.label, pct, counts);
        }
        for n in &p.notes {
            println!("  note: {n}");
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}
