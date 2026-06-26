use crate::model::{Aggregated, ProviderStatus, ProviderUsage, UsageWindow, WindowKey};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use std::time::Instant;

pub fn render(frame: &mut Frame, state: &Aggregated) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, root[0], state);
    render_providers(frame, root[1], state);
    render_footer(frame, root[2], state);
}

fn render_header(frame: &mut Frame, area: Rect, state: &Aggregated) {
    let last = state
        .last_refresh
        .map(|i| format!("{} ago", fmt_duration(i.elapsed())))
        .unwrap_or_else(|| "never".into());
    let next = state
        .next_refresh
        .map(|i| {
            if i > Instant::now() { fmt_duration(i - Instant::now()) } else { "now".into() }
        })
        .unwrap_or_else(|| "n/a".into());
    let title = Line::from(vec![
        Span::styled("ai-usage-dashboard", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  ·  sorted by 5h % used  ·  refreshed "),
        Span::styled(last, Style::default().fg(Color::Green)),
        Span::raw("  ·  next "),
        Span::styled(next, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(Paragraph::new(title).block(Block::default().borders(Borders::ALL)), area);
}

fn render_providers(frame: &mut Frame, area: Rect, state: &Aggregated) {
    let providers = state.sorted_by_usage();
    let constraints = if providers.is_empty() {
        vec![Constraint::Min(1)]
    } else {
        providers.iter().map(|_| Constraint::Length(8)).collect::<Vec<_>>()
    };
    let chunks = Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);

    if providers.is_empty() {
        frame.render_widget(Paragraph::new("No providers configured").block(Block::default().borders(Borders::ALL)), area);
        return;
    }

    for (provider, chunk) in providers.into_iter().zip(chunks.iter()) {
        render_provider(frame, *chunk, provider);
    }
}

fn render_provider(frame: &mut Frame, area: Rect, p: &ProviderUsage) {
    let status_style = match &p.status {
        ProviderStatus::Live => Style::default().fg(Color::Green),
        ProviderStatus::Stale => Style::default().fg(Color::Yellow),
        ProviderStatus::CredentialsMissing => Style::default().fg(Color::DarkGray),
        ProviderStatus::Error { .. } => Style::default().fg(Color::Red),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(format!(" {} ", p.label), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", p.status.chip()), status_style),
            Span::raw(format!(" {} ", p.provider.source())),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(inner);

    let mut windows = p.windows.clone();
    windows.sort_by_key(|w| match w.key { WindowKey::FiveHour => 0, WindowKey::Weekly => 1, WindowKey::Additional(_) => 2 });

    for (idx, row) in rows.iter().take(2).enumerate() {
        let wanted = if idx == 0 { WindowKey::FiveHour } else { WindowKey::Weekly };
        if let Some(w) = windows.iter().find(|w| same_key(&w.key, &wanted)) {
            render_window(frame, *row, w);
        } else {
            frame.render_widget(Paragraph::new(if idx == 0 { "5h: n/a" } else { "Weekly: n/a" }), *row);
        }
    }

    let mut notes: Vec<ListItem> = p.notes.iter().take(3).map(|n| ListItem::new(n.clone())).collect();
    if let ProviderStatus::Error { message } = &p.status {
        notes.insert(0, ListItem::new(Line::from(Span::styled(message.clone(), Style::default().fg(Color::Red)))));
    }
    if notes.is_empty() {
        let top = total_line(p);
        notes.push(ListItem::new(top));
    }
    frame.render_widget(List::new(notes), rows[2]);
}

fn same_key(a: &WindowKey, b: &WindowKey) -> bool {
    matches!((a, b), (WindowKey::FiveHour, WindowKey::FiveHour) | (WindowKey::Weekly, WindowKey::Weekly))
}

fn render_window(frame: &mut Frame, area: Rect, w: &UsageWindow) {
    let pct = w.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let label = format!(
        "{}  {:>5.1}%  {}  {}",
        w.label,
        pct,
        raw_counts(w),
        reset_text(w)
    );
    let color = if pct >= 90.0 { Color::Red } else if pct >= 70.0 { Color::Yellow } else { Color::Green };
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(color))
            .label(label)
            .ratio((pct / 100.0).clamp(0.0, 1.0)),
        area,
    );
}

fn total_line(p: &ProviderUsage) -> String {
    let total: i64 = p.windows.iter().filter_map(|w| w.used_raw).sum();
    if total > 0 { format!("tokens/credits seen: {}", fmt_num(total)) } else { "no token/credit counts exposed for this provider".into() }
}

fn raw_counts(w: &UsageWindow) -> String {
    match (w.used_raw, w.total_raw) {
        (Some(u), Some(t)) if t > 0 => format!("{}/{}", fmt_num(u), fmt_num(t)),
        (Some(u), _) => fmt_num(u),
        _ => "".into(),
    }
}

fn reset_text(w: &UsageWindow) -> String {
    let Some(reset) = w.reset_at else { return "reset n/a".into(); };
    let now = chrono::Utc::now().timestamp();
    if reset <= now { "resets now".into() } else { format!("resets in {}", fmt_secs(reset - now)) }
}

fn render_footer(frame: &mut Frame, area: Rect, state: &Aggregated) {
    let auth = state.auth_source.as_deref().unwrap_or("env only / no auth.json");
    let line = format!("[r] refresh  [q] quit  ·  auth: {}", auth);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}

fn fmt_duration(d: std::time::Duration) -> String { fmt_secs(d.as_secs() as i64) }

fn fmt_secs(secs: i64) -> String {
    if secs < 60 { format!("{}s", secs.max(0)) }
    else if secs < 3600 { format!("{}m", secs / 60) }
    else if secs < 86400 { format!("{}h {}m", secs / 3600, (secs % 3600) / 60) }
    else { format!("{}d {}h", secs / 86400, (secs % 86400) / 3600) }
}

fn fmt_num(n: i64) -> String {
    let abs = n.abs();
    if abs >= 1_000_000_000 { format!("{:.1}B", n as f64 / 1_000_000_000.0) }
    else if abs >= 1_000_000 { format!("{:.1}M", n as f64 / 1_000_000.0) }
    else if abs >= 1_000 { format!("{:.1}K", n as f64 / 1_000.0) }
    else { n.to_string() }
}
